use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::manifest::{ReleaseIdentity, ReleaseManifest, resolve_project_path};

pub fn validate_manifest(identity: &ReleaseIdentity) -> Result<()> {
    identity.validate()
}

pub fn render_formula(identity: &ReleaseIdentity) -> Result<String> {
    validate_manifest(identity)?;
    Ok(format!(
        "class GitSlop < Formula\n\
         \x20 desc \"Deterministic repository health analysis for humans and AI agents\"\n\
         \x20 homepage \"https://github.com/coreycoto/git-slop\"\n\
         \x20 url \"{}\"\n\
         \x20 sha256 \"{}\"\n\
         \x20 license \"MIT\"\n\
         \n\
         \x20 depends_on \"rust\" => :build\n\
         \n\
         \x20 def install\n\
         \x20   system \"cargo\", \"install\", *std_cargo_args\n\
         \x20   man1.install \"man/git-slop.1\"\n\
         \x20   generate_completions_from_executable(bin/\"git-slop\", \"completions\", shells: [:bash, :zsh, :fish])\n\
         \x20 end\n\
         \n\
         \x20 test do\n\
         \x20   assert_match \"git-slop {}\", shell_output(\"#{{bin}}/git-slop version\")\n\
         \x20   build_info = shell_output(\"#{{bin}}/git-slop build-info --format json\")\n\
         \x20   assert_match \"\\\"source_revision\\\": \\\"{}\\\"\", build_info\n\
         \x20   assert_match \"\\\"source_dirty\\\": false\", build_info\n\
         \x20 end\n\
         end\n",
        identity.crate_source.url,
        identity.crate_source.sha256,
        identity.version,
        identity.revision,
    ))
}

pub fn load_manifest(project_root: &Path, path: &Path) -> Result<ReleaseIdentity> {
    let path = resolve_project_path(project_root, path)?;
    let text =
        fs::read_to_string(&path).with_context(|| format!("unable to read {}", path.display()))?;
    let manifest = serde_json::from_str::<ReleaseManifest>(&text)
        .with_context(|| format!("unable to parse {}", path.display()))?;
    manifest.validate()?;
    Ok(manifest.identity())
}

pub fn write_formula(
    project_root: &Path,
    formula_path: &Path,
    identity: &ReleaseIdentity,
) -> Result<PathBuf> {
    let formula_path = resolve_project_path(project_root, formula_path)?;
    if let Some(parent) = formula_path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("unable to create {}", parent.display()))?;
    }
    fs::write(&formula_path, render_formula(identity)?)
        .with_context(|| format!("unable to write {}", formula_path.display()))?;
    Ok(formula_path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crates_io::CrateSource;
    use crate::manifest::{
        CHECKSUM_FILE_NAME, ChecksumMetadata, InstallInstructions, RELEASE_TARGETS, REPO_FULL_NAME,
        ReleaseArtifact, artifact_name,
    };

    fn identity() -> ReleaseIdentity {
        ReleaseIdentity::new(
            "0.9.0",
            "v0.9.0",
            "b".repeat(40),
            CrateSource::new("0.9.0", "a".repeat(64), "b".repeat(40)),
        )
    }

    fn manifest(identity: ReleaseIdentity) -> ReleaseManifest {
        let release_url = format!(
            "https://github.com/{REPO_FULL_NAME}/releases/download/{}",
            identity.tag
        );
        let artifacts = RELEASE_TARGETS
            .into_iter()
            .map(|target| {
                let name = artifact_name(&identity.tag, target);
                ReleaseArtifact {
                    name: name.clone(),
                    path: name.clone(),
                    target: target.target.into(),
                    os: target.os.into(),
                    arch: target.arch.into(),
                    archive: target.archive.into(),
                    sha256: "c".repeat(64),
                    size_bytes: 1,
                    url: format!("{release_url}/{name}"),
                }
            })
            .collect();
        let tag = identity.tag.clone();
        ReleaseManifest::new(
            identity,
            artifacts,
            ChecksumMetadata {
                algorithm: "sha256".into(),
                name: CHECKSUM_FILE_NAME.into(),
                url: format!("{release_url}/{CHECKSUM_FILE_NAME}"),
            },
            InstallInstructions {
                homebrew_tap: vec![
                    "brew tap coreycoto/tap".into(),
                    "brew install coreycoto/tap/git-slop".into(),
                ],
                github_release: vec![
                    format!(
                        "gh release download {tag} --repo {REPO_FULL_NAME} --pattern \
                         'git-slop-{tag}-<target>.*' --pattern {CHECKSUM_FILE_NAME}"
                    ),
                    format!("sha256sum --check {CHECKSUM_FILE_NAME} --ignore-missing"),
                ],
            },
        )
    }

    #[test]
    fn renders_registry_backed_native_rust_formula() -> Result<()> {
        let formula = render_formula(&identity())?;
        let expected = format!(
            "class GitSlop < Formula\n\
             \x20 desc \"Deterministic repository health analysis for humans and AI agents\"\n\
             \x20 homepage \"https://github.com/coreycoto/git-slop\"\n\
             \x20 url \"https://static.crates.io/crates/git-slop/git-slop-0.9.0.crate\"\n\
             \x20 sha256 \"{}\"\n\
             \x20 license \"MIT\"\n\
             \n\
             \x20 depends_on \"rust\" => :build\n\
             \n\
             \x20 def install\n\
             \x20   system \"cargo\", \"install\", *std_cargo_args\n\
             \x20   man1.install \"man/git-slop.1\"\n\
             \x20   generate_completions_from_executable(bin/\"git-slop\", \"completions\", shells: [:bash, :zsh, :fish])\n\
             \x20 end\n\
             \n\
             \x20 test do\n\
             \x20   assert_match \"git-slop 0.9.0\", shell_output(\"#{{bin}}/git-slop version\")\n\
             \x20   build_info = shell_output(\"#{{bin}}/git-slop build-info --format json\")\n\
             \x20   assert_match \"\\\"source_revision\\\": \\\"{}\\\"\", build_info\n\
             \x20   assert_match \"\\\"source_dirty\\\": false\", build_info\n\
             \x20 end\n\
             end\n",
            "a".repeat(64),
            "b".repeat(40),
        );
        assert_eq!(formula, expected);
        assert!(
            formula
                .contains("url \"https://static.crates.io/crates/git-slop/git-slop-0.9.0.crate\"")
        );
        assert!(formula.contains(&format!("sha256 \"{}\"", "a".repeat(64))));
        assert!(formula.contains(&format!(
            "assert_match \"\\\"source_revision\\\": \\\"{}\\\"\", build_info",
            "b".repeat(40)
        )));
        assert!(formula.contains("assert_match \"\\\"source_dirty\\\": false\", build_info"));
        assert!(
            !formula
                .lines()
                .any(|line| line.trim_start().starts_with("version "))
        );
        assert!(!formula.contains("assert_match %"));
        assert!(formula.contains("system \"cargo\", \"install\", *std_cargo_args"));
        for forbidden in ["Python", "python@", "libyaml", "resource ", "tag:"] {
            assert!(!formula.contains(forbidden), "unexpected {forbidden}");
        }
        Ok(())
    }

    #[test]
    fn rejects_release_and_crate_identity_drift() {
        let mut version = identity();
        version.crate_source = CrateSource::new("0.9.1", "a".repeat(64), version.revision.clone());
        assert!(
            validate_manifest(&version)
                .unwrap_err()
                .to_string()
                .contains("version must agree")
        );

        let mut revision = identity();
        revision.crate_source.revision = "c".repeat(40);
        assert!(
            validate_manifest(&revision)
                .unwrap_err()
                .to_string()
                .contains("revision must agree")
        );

        let mut checksum = identity();
        checksum.crate_source.sha256 = "invalid".into();
        assert!(
            validate_manifest(&checksum)
                .unwrap_err()
                .to_string()
                .contains("sha256")
        );
    }

    #[test]
    fn loads_full_release_manifest_and_writes_formula() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let payload = serde_json::to_string_pretty(&manifest(identity()))?;
        fs::write(temp.path().join("release-manifest.json"), payload)?;
        let loaded = load_manifest(temp.path(), Path::new("release-manifest.json"))?;
        assert_eq!(loaded, identity());

        let written = write_formula(temp.path(), Path::new("tap/Formula/git-slop.rb"), &loaded)?;
        assert_eq!(written, temp.path().join("tap/Formula/git-slop.rb"));
        assert_eq!(fs::read_to_string(written)?, render_formula(&loaded)?);
        Ok(())
    }
}
