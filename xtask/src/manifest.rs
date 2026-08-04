use std::collections::BTreeSet;
use std::ffi::OsStr;
use std::fs::{self, File};
use std::io::{BufReader, Read};
use std::path::{Component, Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, anyhow, bail};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const PROJECT_NAME: &str = "git-slop";
pub const REPO_FULL_NAME: &str = "coreycoto/git-slop";
pub const REPO_GIT_URL: &str = "https://github.com/coreycoto/git-slop.git";
pub const MANIFEST_SCHEMA_VERSION: u32 = 2;
pub const CHECKSUM_FILE_NAME: &str = "SHA256SUMS";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReleaseTarget {
    pub target: &'static str,
    pub os: &'static str,
    pub arch: &'static str,
    pub archive: &'static str,
}

/// The complete supported release matrix, kept in deterministic target order.
pub const RELEASE_TARGETS: [ReleaseTarget; 5] = [
    ReleaseTarget {
        target: "aarch64-apple-darwin",
        os: "macos",
        arch: "aarch64",
        archive: "tar.gz",
    },
    ReleaseTarget {
        target: "aarch64-pc-windows-msvc",
        os: "windows",
        arch: "aarch64",
        archive: "zip",
    },
    ReleaseTarget {
        target: "aarch64-unknown-linux-gnu",
        os: "linux",
        arch: "aarch64",
        archive: "tar.gz",
    },
    ReleaseTarget {
        target: "x86_64-pc-windows-msvc",
        os: "windows",
        arch: "x86_64",
        archive: "zip",
    },
    ReleaseTarget {
        target: "x86_64-unknown-linux-gnu",
        os: "linux",
        arch: "x86_64",
        archive: "tar.gz",
    },
];

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommandSpec {
    pub program: String,
    pub args: Vec<String>,
}

impl CommandSpec {
    pub fn new<I, S>(program: impl Into<String>, args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self {
            program: program.into(),
            args: args.into_iter().map(Into::into).collect(),
        }
    }

    pub fn display(&self) -> String {
        std::iter::once(self.program.as_str())
            .chain(self.args.iter().map(String::as_str))
            .collect::<Vec<_>>()
            .join(" ")
    }
}

/// Injectable command boundary used by release orchestration and exact Git lookups.
pub trait CommandRunner {
    fn output(&mut self, cwd: &Path, command: &CommandSpec) -> Result<String>;
    fn run(&mut self, cwd: &Path, command: &CommandSpec) -> Result<()>;
}

#[derive(Clone, Copy, Debug, Default)]
pub struct SystemCommandRunner;

impl CommandRunner for SystemCommandRunner {
    fn output(&mut self, cwd: &Path, command: &CommandSpec) -> Result<String> {
        let output = Command::new(&command.program)
            .args(&command.args)
            .current_dir(cwd)
            .output()
            .with_context(|| format!("unable to run {}", command.display()))?;
        if !output.status.success() {
            let detail = command_failure_detail(&output.stdout, &output.stderr);
            bail!("{} failed{}", command.display(), detail);
        }
        String::from_utf8(output.stdout)
            .with_context(|| format!("{} produced non-UTF-8 output", command.display()))
            .map(|output| output.trim().to_owned())
    }

    fn run(&mut self, cwd: &Path, command: &CommandSpec) -> Result<()> {
        let status = Command::new(&command.program)
            .args(&command.args)
            .current_dir(cwd)
            .status()
            .with_context(|| format!("unable to run {}", command.display()))?;
        if !status.success() {
            bail!("{} failed with {status}", command.display());
        }
        Ok(())
    }
}

fn command_failure_detail(stdout: &[u8], stderr: &[u8]) -> String {
    let stdout = String::from_utf8_lossy(stdout);
    let stderr = String::from_utf8_lossy(stderr);
    let detail = format!("{}{}", stdout.trim(), stderr.trim());
    if detail.is_empty() {
        String::new()
    } else {
        format!(": {detail}")
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct HomebrewSource {
    pub url: String,
    pub tag: String,
    pub revision: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ReleaseIdentity {
    pub schema_version: u32,
    pub project: String,
    pub version: String,
    pub tag: String,
    pub revision: String,
    pub repository: String,
    pub homebrew_source: HomebrewSource,
}

impl ReleaseIdentity {
    pub fn new(
        version: impl Into<String>,
        tag: impl Into<String>,
        revision: impl Into<String>,
    ) -> Self {
        let version = version.into();
        let tag = tag.into();
        let revision = revision.into();
        Self {
            schema_version: MANIFEST_SCHEMA_VERSION,
            project: PROJECT_NAME.to_owned(),
            version,
            tag: tag.clone(),
            revision: revision.clone(),
            repository: REPO_FULL_NAME.to_owned(),
            homebrew_source: HomebrewSource {
                url: REPO_GIT_URL.to_owned(),
                tag,
                revision,
            },
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ReleaseArtifact {
    pub name: String,
    pub path: String,
    pub target: String,
    pub os: String,
    pub arch: String,
    pub archive: String,
    pub sha256: String,
    pub size_bytes: u64,
    pub url: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ChecksumMetadata {
    pub algorithm: String,
    pub name: String,
    pub url: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct InstallInstructions {
    pub homebrew_tap: Vec<String>,
    pub github_release: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ReleaseManifest {
    #[serde(flatten)]
    pub identity: ReleaseIdentity,
    pub artifacts: Vec<ReleaseArtifact>,
    pub checksums: ChecksumMetadata,
    pub install: InstallInstructions,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ManifestOutputPaths {
    pub manifest: PathBuf,
    pub checksums: PathBuf,
}

pub fn is_strict_semver(version: &str) -> bool {
    let mut parts = version.split('.');
    let valid_part = |part: &str| {
        !part.is_empty()
            && part.bytes().all(|byte| byte.is_ascii_digit())
            && (part == "0" || !part.starts_with('0'))
    };
    parts.by_ref().take(3).all(valid_part)
        && version.matches('.').count() == 2
        && parts.next().is_none()
}

pub fn strict_tag_version(tag: &str) -> Result<&str> {
    let Some(version) = tag.strip_prefix('v') else {
        bail!("release tag must be strict semver in vX.Y.Z form: {tag}");
    };
    if !is_strict_semver(version) {
        bail!("release tag must be strict semver in vX.Y.Z form: {tag}");
    }
    Ok(version)
}

pub fn project_version(project_root: &Path) -> Result<String> {
    let cargo_path = project_root.join("Cargo.toml");
    let text = fs::read_to_string(&cargo_path)
        .with_context(|| format!("unable to read {}", cargo_path.display()))?;
    let payload: toml::Value = toml::from_str(&text)
        .with_context(|| format!("unable to parse {}", cargo_path.display()))?;
    payload
        .get("package")
        .and_then(|package| package.get("version"))
        .and_then(toml::Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| anyhow!("Cargo.toml must define package.version"))
}

pub fn git_revision(project_root: &Path, release_tag: &str) -> Result<String> {
    git_revision_with_runner(project_root, release_tag, &mut SystemCommandRunner)
}

pub fn git_revision_with_runner(
    project_root: &Path,
    release_tag: &str,
    runner: &mut impl CommandRunner,
) -> Result<String> {
    let command = CommandSpec::new(
        "git",
        [
            "rev-parse".to_owned(),
            "--verify".to_owned(),
            format!("refs/tags/{release_tag}^{{commit}}"),
        ],
    );
    runner.output(project_root, &command)
}

pub fn artifact_name(tag: &str, target: ReleaseTarget) -> String {
    format!("git-slop-{tag}-{}.{}", target.target, target.archive)
}

pub fn build_manifest(
    project_root: &Path,
    dist_dir: &Path,
    tag: Option<&str>,
) -> Result<ReleaseManifest> {
    build_manifest_with_runner(project_root, dist_dir, tag, &mut SystemCommandRunner)
}

pub fn build_manifest_with_runner(
    project_root: &Path,
    dist_dir: &Path,
    tag: Option<&str>,
    runner: &mut impl CommandRunner,
) -> Result<ReleaseManifest> {
    let version = project_version(project_root)?;
    let release_tag = tag.map_or_else(|| format!("v{version}"), str::to_owned);
    let tag_version = strict_tag_version(&release_tag)?;
    if tag_version != version {
        bail!("Cargo.toml version is {version}; release tag {release_tag} is {tag_version}.");
    }
    if !dist_dir.is_dir() {
        bail!("dist dir does not exist: {}", dist_dir.display());
    }

    let artifacts = release_artifacts(dist_dir, &release_tag)?;
    let revision = git_revision_with_runner(project_root, &release_tag, runner)?;
    let release_url =
        format!("https://github.com/{REPO_FULL_NAME}/releases/download/{release_tag}");
    Ok(ReleaseManifest {
        identity: ReleaseIdentity::new(version, &release_tag, revision),
        artifacts,
        checksums: ChecksumMetadata {
            algorithm: "sha256".to_owned(),
            name: CHECKSUM_FILE_NAME.to_owned(),
            url: format!("{release_url}/{CHECKSUM_FILE_NAME}"),
        },
        install: InstallInstructions {
            homebrew_tap: vec![
                "brew tap coreycoto/tap".to_owned(),
                "brew install coreycoto/tap/git-slop".to_owned(),
            ],
            github_release: vec![
                format!(
                    "gh release download {release_tag} --repo {REPO_FULL_NAME} --pattern \
                     'git-slop-{release_tag}-<target>.*' --pattern {CHECKSUM_FILE_NAME}"
                ),
                format!("sha256sum --check {CHECKSUM_FILE_NAME} --ignore-missing"),
            ],
        },
    })
}

fn release_artifacts(dist_dir: &Path, tag: &str) -> Result<Vec<ReleaseArtifact>> {
    let release_url = format!("https://github.com/{REPO_FULL_NAME}/releases/download/{tag}");
    let mut artifacts = Vec::with_capacity(RELEASE_TARGETS.len());
    let mut missing = Vec::new();

    for target in RELEASE_TARGETS {
        let name = artifact_name(tag, target);
        let path = dist_dir.join(&name);
        if !path.is_file() {
            missing.push(name);
            continue;
        }
        let metadata =
            fs::metadata(&path).with_context(|| format!("unable to inspect {}", path.display()))?;
        artifacts.push(ReleaseArtifact {
            name: name.clone(),
            path: name.clone(),
            target: target.target.to_owned(),
            os: target.os.to_owned(),
            arch: target.arch.to_owned(),
            archive: target.archive.to_owned(),
            sha256: sha256_file(&path)?,
            size_bytes: metadata.len(),
            url: format!("{release_url}/{name}"),
        });
    }

    if !missing.is_empty() {
        bail!(
            "missing required release artifact(s): {}",
            missing.join(", ")
        );
    }

    let expected_names = artifacts
        .iter()
        .map(|artifact| artifact.name.as_str())
        .collect::<BTreeSet<_>>();
    let prefix = format!("git-slop-{tag}-");
    let mut unexpected = Vec::new();
    for entry in
        fs::read_dir(dist_dir).with_context(|| format!("unable to list {}", dist_dir.display()))?
    {
        let entry = entry.with_context(|| format!("unable to inspect {}", dist_dir.display()))?;
        if !entry.path().is_file() {
            continue;
        }
        let name = entry.file_name().into_string().map_err(|name| {
            anyhow!(
                "release distribution contains a non-UTF-8 filename: {:?}",
                name
            )
        })?;
        if name.starts_with(&prefix) && !expected_names.contains(name.as_str()) {
            unexpected.push(name);
        }
    }
    unexpected.sort();
    if !unexpected.is_empty() {
        bail!("unexpected release artifact(s): {}", unexpected.join(", "));
    }

    Ok(artifacts)
}

pub fn sha256_file(path: &Path) -> Result<String> {
    let file = File::open(path).with_context(|| format!("unable to read {}", path.display()))?;
    let mut reader = BufReader::new(file);
    let mut digest = Sha256::new();
    let mut buffer = [0_u8; 1024 * 1024];
    loop {
        let count = reader
            .read(&mut buffer)
            .with_context(|| format!("unable to read {}", path.display()))?;
        if count == 0 {
            break;
        }
        digest.update(&buffer[..count]);
    }
    Ok(hex::encode(digest.finalize()))
}

pub fn checksum_lines(artifacts: &[ReleaseArtifact]) -> String {
    let mut lines = artifacts
        .iter()
        .map(|artifact| format!("{}  {}\n", artifact.sha256, artifact.name))
        .collect::<Vec<_>>();
    lines.sort();
    lines.concat()
}

pub fn render_manifest_json(manifest: &ReleaseManifest) -> Result<String> {
    // serde_json::Value uses a sorted map without preserve_order, matching the
    // previous recursive sort_keys=True output rather than Rust field order.
    let value = serde_json::to_value(manifest).context("unable to serialize release manifest")?;
    let mut rendered =
        serde_json::to_string_pretty(&value).context("unable to render release manifest JSON")?;
    rendered.push('\n');
    Ok(rendered)
}

pub fn write_manifest_outputs(
    project_root: &Path,
    dist_dir: &Path,
    manifest: &ReleaseManifest,
    output: &Path,
    checksum_output: &Path,
) -> Result<ManifestOutputPaths> {
    let dist_dir = resolve_project_path(project_root, dist_dir)?;
    let output = resolve_project_path(project_root, output)?;
    let checksum_output = resolve_project_path(project_root, checksum_output)?;
    if output == checksum_output {
        bail!(
            "release manifest and checksum outputs must be different paths: {}",
            output.display()
        );
    }
    for artifact in &manifest.artifacts {
        let source = resolve_project_path(project_root, &dist_dir.join(&artifact.path))?;
        if output == source || checksum_output == source {
            bail!(
                "release metadata output must not overwrite source archive {}",
                source.display()
            );
        }
    }
    create_parent(&output)?;
    create_parent(&checksum_output)?;
    fs::write(&output, render_manifest_json(manifest)?)
        .with_context(|| format!("unable to write {}", output.display()))?;
    fs::write(&checksum_output, checksum_lines(&manifest.artifacts))
        .with_context(|| format!("unable to write {}", checksum_output.display()))?;
    Ok(ManifestOutputPaths {
        manifest: output,
        checksums: checksum_output,
    })
}

pub fn resolve_project_path(project_root: &Path, path: &Path) -> Result<PathBuf> {
    let joined = if path.is_absolute() {
        path.to_path_buf()
    } else {
        project_root.join(path)
    };
    lexical_absolute(&joined)
}

fn lexical_absolute(path: &Path) -> Result<PathBuf> {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .context("unable to resolve current directory")?
            .join(path)
    };
    let mut normalized = PathBuf::new();
    for component in absolute.components() {
        match component {
            Component::Prefix(prefix) => normalized.push(prefix.as_os_str()),
            Component::RootDir => normalized.push(OsStr::new(std::path::MAIN_SEPARATOR_STR)),
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            Component::Normal(part) => normalized.push(part),
        }
    }
    Ok(normalized)
}

fn create_parent(path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("unable to create {}", parent.display()))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;

    use tempfile::TempDir;

    use super::*;

    #[derive(Default)]
    struct FakeRunner {
        outputs: VecDeque<String>,
        output_calls: Vec<(PathBuf, CommandSpec)>,
        run_calls: Vec<(PathBuf, CommandSpec)>,
    }

    impl CommandRunner for FakeRunner {
        fn output(&mut self, cwd: &Path, command: &CommandSpec) -> Result<String> {
            self.output_calls.push((cwd.to_path_buf(), command.clone()));
            self.outputs
                .pop_front()
                .ok_or_else(|| anyhow!("missing fake output"))
        }

        fn run(&mut self, cwd: &Path, command: &CommandSpec) -> Result<()> {
            self.run_calls.push((cwd.to_path_buf(), command.clone()));
            Ok(())
        }
    }

    fn fixture() -> Result<(TempDir, PathBuf)> {
        let temp = tempfile::tempdir()?;
        fs::write(
            temp.path().join("Cargo.toml"),
            "[package]\nname = \"git-slop\"\nversion = \"0.9.0\"\n",
        )?;
        let dist = temp.path().join("dist");
        fs::create_dir(&dist)?;
        for target in RELEASE_TARGETS {
            fs::write(
                dist.join(artifact_name("v0.9.0", target)),
                format!("{}\n", target.target),
            )?;
        }
        Ok((temp, dist))
    }

    #[test]
    fn strict_semver_matches_stable_contract() {
        for valid in ["0.0.0", "0.9.0", "10.20.300"] {
            assert!(is_strict_semver(valid), "{valid}");
        }
        for invalid in [
            "v0.9.0",
            "01.2.3",
            "1.02.3",
            "1.2.03",
            "1.2",
            "1.2.3.4",
            "1.2.3-rc.1",
            "1.a.3",
            "",
        ] {
            assert!(!is_strict_semver(invalid), "{invalid}");
        }
    }

    #[test]
    fn exact_release_matrix_builds_deterministic_manifest() -> Result<()> {
        let (temp, dist) = fixture()?;
        let mut runner = FakeRunner {
            outputs: VecDeque::from(["aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into()]),
            ..FakeRunner::default()
        };
        let manifest = build_manifest_with_runner(temp.path(), &dist, Some("v0.9.0"), &mut runner)?;

        assert_eq!(manifest.identity.schema_version, 2);
        assert_eq!(manifest.identity.project, PROJECT_NAME);
        assert_eq!(manifest.identity.repository, REPO_FULL_NAME);
        assert_eq!(manifest.artifacts.len(), RELEASE_TARGETS.len());
        assert_eq!(
            manifest
                .artifacts
                .iter()
                .map(|artifact| artifact.target.as_str())
                .collect::<Vec<_>>(),
            RELEASE_TARGETS
                .iter()
                .map(|target| target.target)
                .collect::<Vec<_>>()
        );
        assert!(manifest.artifacts.iter().all(|artifact| {
            artifact.url
                == format!(
                    "https://github.com/{REPO_FULL_NAME}/releases/download/v0.9.0/{}",
                    artifact.name
                )
        }));
        assert_eq!(runner.output_calls.len(), 1);
        assert_eq!(
            runner.output_calls[0].1,
            CommandSpec::new(
                "git",
                ["rev-parse", "--verify", "refs/tags/v0.9.0^{commit}",]
            )
        );

        let checksums = checksum_lines(&manifest.artifacts);
        assert_eq!(
            checksums,
            include_str!("../tests/fixtures/SHA256SUMS-v0.9.0")
        );

        let json = render_manifest_json(&manifest)?;
        assert_eq!(
            json,
            include_str!("../tests/fixtures/release-manifest-v0.9.0.json")
        );
        assert_eq!(json, render_manifest_json(&manifest)?);
        Ok(())
    }

    #[test]
    fn missing_and_unexpected_release_artifacts_fail_closed() -> Result<()> {
        let (temp, dist) = fixture()?;
        let missing = artifact_name("v0.9.0", RELEASE_TARGETS[1]);
        fs::remove_file(dist.join(&missing))?;
        let error = build_manifest_with_runner(
            temp.path(),
            &dist,
            Some("v0.9.0"),
            &mut FakeRunner::default(),
        )
        .unwrap_err();
        assert!(
            error
                .to_string()
                .contains("missing required release artifact")
        );

        fs::write(dist.join(&missing), b"restored\n")?;
        fs::write(
            dist.join("git-slop-v0.9.0-riscv64gc-unknown-linux-gnu.tar.gz"),
            b"unsupported\n",
        )?;
        let error = build_manifest_with_runner(
            temp.path(),
            &dist,
            Some("v0.9.0"),
            &mut FakeRunner::default(),
        )
        .unwrap_err();
        assert!(error.to_string().contains("unexpected release artifact"));
        Ok(())
    }

    #[test]
    fn version_and_tag_must_agree_before_git_resolution() -> Result<()> {
        let (temp, dist) = fixture()?;
        let mut runner = FakeRunner::default();
        let error = build_manifest_with_runner(temp.path(), &dist, Some("v0.9.1"), &mut runner)
            .unwrap_err();
        assert_eq!(
            error.to_string(),
            "Cargo.toml version is 0.9.0; release tag v0.9.1 is 0.9.1."
        );
        assert!(runner.output_calls.is_empty());
        Ok(())
    }

    #[test]
    fn output_files_preserve_final_newlines() -> Result<()> {
        let (temp, dist) = fixture()?;
        let mut runner = FakeRunner {
            outputs: VecDeque::from(["aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into()]),
            ..FakeRunner::default()
        };
        let manifest = build_manifest_with_runner(temp.path(), &dist, Some("v0.9.0"), &mut runner)?;
        let paths = write_manifest_outputs(
            temp.path(),
            &dist,
            &manifest,
            Path::new("generated/release-manifest.json"),
            Path::new("generated/SHA256SUMS"),
        )?;

        assert!(fs::read_to_string(paths.manifest)?.ends_with('\n'));
        assert!(fs::read_to_string(paths.checksums)?.ends_with('\n'));
        Ok(())
    }

    #[test]
    fn output_paths_cannot_collide_with_each_other_or_source_archives() -> Result<()> {
        let (temp, dist) = fixture()?;
        let mut runner = FakeRunner {
            outputs: VecDeque::from(["aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into()]),
            ..FakeRunner::default()
        };
        let manifest = build_manifest_with_runner(temp.path(), &dist, Some("v0.9.0"), &mut runner)?;
        let shared = Path::new("generated/release-metadata");
        let error =
            write_manifest_outputs(temp.path(), &dist, &manifest, shared, shared).unwrap_err();
        assert!(error.to_string().contains("must be different paths"));
        assert!(!temp.path().join(shared).exists());

        let archive = dist.join(&manifest.artifacts[0].name);
        let original = fs::read(&archive)?;
        let error = write_manifest_outputs(
            temp.path(),
            &dist,
            &manifest,
            &archive,
            Path::new("generated/SHA256SUMS"),
        )
        .unwrap_err();
        assert!(
            error
                .to_string()
                .contains("must not overwrite source archive")
        );
        assert_eq!(fs::read(archive)?, original);
        Ok(())
    }
}
