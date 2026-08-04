use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

use crate::manifest::{
    CommandRunner, MANIFEST_SCHEMA_VERSION, PROJECT_NAME, REPO_FULL_NAME, REPO_GIT_URL,
    ReleaseIdentity, SystemCommandRunner, git_revision_with_runner, is_strict_semver,
    resolve_project_path,
};

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct FormulaSourceArgs {
    pub manifest: Option<PathBuf>,
    pub tag: Option<String>,
    pub version: Option<String>,
    pub revision: Option<String>,
}

pub fn validate_manifest(identity: &ReleaseIdentity) -> Result<()> {
    if identity.schema_version != MANIFEST_SCHEMA_VERSION {
        bail!("release manifest schema_version must be 2");
    }
    if identity.project != PROJECT_NAME {
        bail!("release manifest project must be git-slop");
    }
    if identity.repository != REPO_FULL_NAME {
        bail!("release manifest repository must be {REPO_FULL_NAME}");
    }
    if !is_strict_semver(&identity.version) {
        bail!("release manifest version must be strict semver");
    }
    if identity.tag != format!("v{}", identity.version) {
        bail!("release manifest tag must agree with version");
    }
    if !is_full_revision(&identity.revision) {
        bail!("release manifest revision must be a full commit id");
    }
    if identity.homebrew_source.url != REPO_GIT_URL {
        bail!("homebrew_source url must be {REPO_GIT_URL}");
    }
    if identity.homebrew_source.tag != identity.tag {
        bail!("homebrew_source tag must agree with the top-level tag");
    }
    if identity.homebrew_source.revision != identity.revision {
        bail!("homebrew_source revision must agree with the top-level revision");
    }
    Ok(())
}

fn is_full_revision(revision: &str) -> bool {
    matches!(revision.len(), 40 | 64) && revision.bytes().all(|byte| byte.is_ascii_hexdigit())
}

pub fn render_formula(identity: &ReleaseIdentity) -> Result<String> {
    validate_manifest(identity)?;
    Ok(format!(
        "class GitSlop < Formula\n\
         \x20 desc \"Deterministic repository health analysis for humans and AI agents\"\n\
         \x20 homepage \"https://github.com/coreycoto/git-slop\"\n\
         \x20 url \"{}\",\n\
         \x20     tag:      \"{}\",\n\
         \x20     revision: \"{}\"\n\
         \x20 license \"MIT\"\n\
         \n\
         \x20 depends_on \"rust\" => :build\n\
         \n\
         \x20 def install\n\
         \x20   system \"cargo\", \"install\", *std_cargo_args\n\
         \x20   man1.install \"man/git-slop.1\"\n\
         \x20 end\n\
         \n\
         \x20 test do\n\
         \x20   assert_match \"git-slop {}\", shell_output(\"#{{bin}}/git-slop version\")\n\
         \x20 end\n\
         end\n",
        identity.homebrew_source.url,
        identity.homebrew_source.tag,
        identity.homebrew_source.revision,
        identity.version,
    ))
}

pub fn load_manifest(project_root: &Path, path: &Path) -> Result<ReleaseIdentity> {
    let path = resolve_project_path(project_root, path)?;
    let text =
        fs::read_to_string(&path).with_context(|| format!("unable to read {}", path.display()))?;
    let identity = serde_json::from_str::<ReleaseIdentity>(&text)
        .with_context(|| format!("unable to parse {}", path.display()))?;
    validate_manifest(&identity)?;
    Ok(identity)
}

pub fn resolve_formula_source(
    project_root: &Path,
    args: &FormulaSourceArgs,
) -> Result<ReleaseIdentity> {
    resolve_formula_source_with_runner(project_root, args, &mut SystemCommandRunner)
}

pub fn resolve_formula_source_with_runner(
    project_root: &Path,
    args: &FormulaSourceArgs,
    runner: &mut impl CommandRunner,
) -> Result<ReleaseIdentity> {
    if let Some(manifest) = &args.manifest {
        return load_manifest(project_root, manifest);
    }

    let (Some(tag), Some(version)) = (&args.tag, &args.version) else {
        bail!("provide --manifest or both --tag and --version");
    };
    let tag_revision = git_revision_with_runner(project_root, tag, runner)
        .with_context(|| format!("unable to resolve release tag {tag}"))?;
    if let Some(revision) = &args.revision {
        if revision != &tag_revision {
            bail!("provided revision {revision} does not match exact tag revision {tag_revision}");
        }
    }
    let revision = tag_revision;
    let identity = ReleaseIdentity::new(version, tag, revision);
    validate_manifest(&identity)?;
    Ok(identity)
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
    use std::collections::VecDeque;

    use anyhow::anyhow;

    use crate::manifest::{CommandSpec, HomebrewSource};

    use super::*;

    #[derive(Default)]
    struct FakeRunner {
        outputs: VecDeque<String>,
        calls: Vec<(PathBuf, CommandSpec)>,
    }

    impl CommandRunner for FakeRunner {
        fn output(&mut self, cwd: &Path, command: &CommandSpec) -> Result<String> {
            self.calls.push((cwd.to_path_buf(), command.clone()));
            self.outputs
                .pop_front()
                .ok_or_else(|| anyhow!("missing fake output"))
        }

        fn run(&mut self, _cwd: &Path, _command: &CommandSpec) -> Result<()> {
            Ok(())
        }
    }

    fn identity() -> ReleaseIdentity {
        ReleaseIdentity::new(
            "0.9.0",
            "v0.9.0",
            "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
        )
    }

    #[test]
    fn renders_exact_native_rust_formula() -> Result<()> {
        let expected = concat!(
            "class GitSlop < Formula\n",
            "  desc \"Deterministic repository health analysis for humans and AI agents\"\n",
            "  homepage \"https://github.com/coreycoto/git-slop\"\n",
            "  url \"https://github.com/coreycoto/git-slop.git\",\n",
            "      tag:      \"v0.9.0\",\n",
            "      revision: \"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb\"\n",
            "  license \"MIT\"\n\n",
            "  depends_on \"rust\" => :build\n\n",
            "  def install\n",
            "    system \"cargo\", \"install\", *std_cargo_args\n",
            "    man1.install \"man/git-slop.1\"\n",
            "  end\n\n",
            "  test do\n",
            "    assert_match \"git-slop 0.9.0\", shell_output(\"#{bin}/git-slop version\")\n",
            "  end\n",
            "end\n",
        );
        let formula = render_formula(&identity())?;
        assert_eq!(formula, expected);
        for forbidden in [
            "Python",
            "python@",
            "libyaml",
            "resource ",
            "depends_on arch:",
        ] {
            assert!(!formula.contains(forbidden));
        }
        Ok(())
    }

    #[test]
    fn rejects_manifest_identity_drift() {
        let mut cases = Vec::new();

        let mut schema = identity();
        schema.schema_version = 1;
        cases.push((schema, "schema_version"));

        let mut project = identity();
        project.project = "another-project".into();
        cases.push((project, "project must be git-slop"));

        let mut repository = identity();
        repository.repository = "someone/else".into();
        cases.push((repository, "repository must be coreycoto/git-slop"));

        let mut version = identity();
        version.version = "01.9.0".into();
        cases.push((version, "version must be strict semver"));

        let mut tag = identity();
        tag.tag = "v0.9.1".into();
        cases.push((tag, "tag must agree"));

        let mut revision = identity();
        revision.revision = "short".into();
        cases.push((revision, "revision must be a full commit id"));

        let mut source_url = identity();
        source_url.homebrew_source.url = "https://example.com/repo.git".into();
        cases.push((source_url, "homebrew_source url"));

        let mut source_tag = identity();
        source_tag.homebrew_source.tag = "v0.9.1".into();
        cases.push((source_tag, "tag must agree"));

        let mut source_revision = identity();
        source_revision.homebrew_source.revision = "c".repeat(40);
        cases.push((source_revision, "revision must agree"));

        for (manifest, expected) in cases {
            let error = render_formula(&manifest).unwrap_err();
            assert!(error.to_string().contains(expected), "{error:#}");
        }
    }

    #[test]
    fn arguments_resolve_only_the_exact_tag_ref() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let mut runner = FakeRunner {
            outputs: VecDeque::from(["cccccccccccccccccccccccccccccccccccccccc".into()]),
            ..FakeRunner::default()
        };
        let args = FormulaSourceArgs {
            tag: Some("v0.9.0".into()),
            version: Some("0.9.0".into()),
            ..FormulaSourceArgs::default()
        };
        let resolved = resolve_formula_source_with_runner(temp.path(), &args, &mut runner)?;

        assert_eq!(resolved.revision, "c".repeat(40));
        assert_eq!(runner.calls.len(), 1);
        assert_eq!(
            runner.calls[0].1,
            CommandSpec::new(
                "git",
                ["rev-parse", "--verify", "refs/tags/v0.9.0^{commit}",]
            )
        );
        Ok(())
    }

    #[test]
    fn explicit_revision_must_match_the_exact_tag_ref() {
        let temp = tempfile::tempdir().unwrap();
        let mut runner = FakeRunner {
            outputs: VecDeque::from(["c".repeat(40)]),
            ..FakeRunner::default()
        };
        let args = FormulaSourceArgs {
            tag: Some("v0.9.0".into()),
            version: Some("0.9.0".into()),
            revision: Some("d".repeat(40)),
            ..FormulaSourceArgs::default()
        };
        let error =
            resolve_formula_source_with_runner(temp.path(), &args, &mut runner).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("does not match exact tag revision")
        );
        assert_eq!(
            runner.calls[0].1,
            CommandSpec::new(
                "git",
                ["rev-parse", "--verify", "refs/tags/v0.9.0^{commit}",]
            )
        );
    }

    #[test]
    fn revision_length_accepts_only_full_sha1_or_sha256_ids() {
        for length in [40, 64] {
            let candidate = ReleaseIdentity::new("0.9.0", "v0.9.0", "a".repeat(length));
            assert!(validate_manifest(&candidate).is_ok(), "length {length}");
        }
        for length in [39, 41, 63, 65] {
            let candidate = ReleaseIdentity::new("0.9.0", "v0.9.0", "a".repeat(length));
            assert!(validate_manifest(&candidate).is_err(), "length {length}");
        }
    }

    #[test]
    fn manifest_input_takes_precedence_and_allows_full_release_payload() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let path = temp.path().join("release-manifest.json");
        let value = serde_json::json!({
            "schema_version": 2,
            "project": "git-slop",
            "repository": "coreycoto/git-slop",
            "version": "0.9.0",
            "tag": "v0.9.0",
            "revision": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
            "homebrew_source": {
                "url": "https://github.com/coreycoto/git-slop.git",
                "tag": "v0.9.0",
                "revision": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
            },
            "artifacts": [],
            "checksums": {},
            "install": {}
        });
        fs::write(&path, serde_json::to_vec(&value)?)?;
        let args = FormulaSourceArgs {
            manifest: Some(PathBuf::from("release-manifest.json")),
            tag: Some("ignored".into()),
            version: Some("ignored".into()),
            revision: Some("ignored".into()),
        };
        let resolved =
            resolve_formula_source_with_runner(temp.path(), &args, &mut FakeRunner::default())?;
        assert_eq!(resolved, identity());
        Ok(())
    }

    #[test]
    fn formula_writes_relative_to_project_root() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let written = write_formula(
            temp.path(),
            Path::new("tap/Formula/git-slop.rb"),
            &identity(),
        )?;
        assert_eq!(written, temp.path().join("tap/Formula/git-slop.rb"));
        assert_eq!(fs::read_to_string(written)?, render_formula(&identity())?);
        Ok(())
    }

    #[test]
    fn incomplete_source_arguments_fail_before_git() {
        let temp = tempfile::tempdir().unwrap();
        let mut runner = FakeRunner::default();
        let error = resolve_formula_source_with_runner(
            temp.path(),
            &FormulaSourceArgs {
                tag: Some("v0.9.0".into()),
                ..FormulaSourceArgs::default()
            },
            &mut runner,
        )
        .unwrap_err();
        assert_eq!(
            error.to_string(),
            "provide --manifest or both --tag and --version"
        );
        assert!(runner.calls.is_empty());
    }

    #[test]
    fn source_url_constant_matches_constructed_identity() {
        let identity = ReleaseIdentity {
            homebrew_source: HomebrewSource {
                url: REPO_GIT_URL.into(),
                tag: "v0.9.0".into(),
                revision: "b".repeat(40),
            },
            ..identity()
        };
        assert!(validate_manifest(&identity).is_ok());
    }
}
