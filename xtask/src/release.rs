use std::path::{Path, PathBuf};

use anyhow::{Result, bail};

use crate::manifest::{
    CommandRunner, CommandSpec, SystemCommandRunner, is_strict_semver, project_version,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReleaseState {
    pub tag: String,
    pub revision: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PrepareReleaseOptions {
    pub project_root: PathBuf,
    pub version: String,
}

impl PrepareReleaseOptions {
    pub fn new(project_root: impl Into<PathBuf>, version: impl Into<String>) -> Self {
        Self {
            project_root: project_root.into(),
            version: version.into(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PrepareReleaseResult {
    pub state: ReleaseState,
    pub commands: Vec<CommandSpec>,
    pub messages: Vec<String>,
}

pub fn validate_project_version(project_root: &Path, expected_version: &str) -> Result<()> {
    if !is_strict_semver(expected_version) {
        bail!("release version must be strict semver in X.Y.Z form: {expected_version}");
    }
    let actual_version = project_version(project_root)?;
    if actual_version != expected_version {
        bail!("Cargo.toml version is {actual_version}; expected {expected_version}.");
    }
    Ok(())
}

pub fn head_revision(project_root: &Path) -> Result<String> {
    head_revision_with_runner(project_root, &mut SystemCommandRunner)
}

pub fn head_revision_with_runner(
    project_root: &Path,
    runner: &mut impl CommandRunner,
) -> Result<String> {
    runner.output(
        project_root,
        &CommandSpec::new("git", ["rev-parse", "HEAD"]),
    )
}

/// Validate a release candidate without requiring or creating its future tag.
///
/// Exact tag-to-revision identity is enforced later by `release-manifest`, after
/// publication has supplied the canonical crates.io package metadata.
pub fn validate_release_state(project_root: &Path, version: &str) -> Result<ReleaseState> {
    validate_release_state_with_runner(project_root, version, &mut SystemCommandRunner)
}

pub fn validate_release_state_with_runner(
    project_root: &Path,
    version: &str,
    runner: &mut impl CommandRunner,
) -> Result<ReleaseState> {
    validate_project_version(project_root, version)?;
    let worktree_status = runner.output(
        project_root,
        &CommandSpec::new("git", ["status", "--porcelain=v1", "--untracked-files=all"]),
    )?;
    if !worktree_status.is_empty() {
        bail!(
            "release candidate worktree must be clean; commit, ignore, or remove every modified and untracked path before validation"
        );
    }
    let revision = head_revision_with_runner(project_root, runner)?;
    if !crate::manifest::is_full_revision(&revision) {
        bail!("release candidate HEAD must be a full commit id");
    }
    let tag = format!("v{version}");
    let tag_output = runner.output(
        project_root,
        &CommandSpec::new(
            "git",
            [
                "for-each-ref".to_owned(),
                "--format=%(*objectname)%09%(objectname)".to_owned(),
                format!("refs/tags/{tag}"),
            ],
        ),
    )?;
    if let Some(tag_revision) = tag_output.split_whitespace().next() {
        if tag_revision != revision {
            bail!(
                "existing release tag {tag} resolves to {tag_revision}, but candidate HEAD is {revision}"
            );
        }
    }
    Ok(ReleaseState { tag, revision })
}

/// Commands run by release preparation, in execution order.
///
/// The public crate and private xtask are deliberately separate workspaces, so
/// each gate names its intended manifest or package. The only publication
/// command is credential-free `--dry-run`; preparation never tags, publishes,
/// or writes another repository.
pub fn release_validation_commands() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("cargo", ["fmt", "--all", "--", "--check"]),
        CommandSpec::new(
            "cargo",
            [
                "fmt",
                "--manifest-path",
                "xtask/Cargo.toml",
                "--all",
                "--",
                "--check",
            ],
        ),
        CommandSpec::new(
            "cargo",
            [
                "clippy",
                "-p",
                "git-slop",
                "--all-targets",
                "--all-features",
                "--locked",
                "--",
                "-D",
                "warnings",
            ],
        ),
        CommandSpec::new(
            "cargo",
            [
                "clippy",
                "--manifest-path",
                "xtask/Cargo.toml",
                "--all-targets",
                "--all-features",
                "--locked",
                "--",
                "-D",
                "warnings",
            ],
        ),
        CommandSpec::new(
            "cargo",
            [
                "test",
                "-p",
                "git-slop",
                "--all-targets",
                "--all-features",
                "--locked",
            ],
        ),
        CommandSpec::new(
            "cargo",
            [
                "test",
                "--manifest-path",
                "xtask/Cargo.toml",
                "--all-targets",
                "--all-features",
                "--locked",
            ],
        ),
        CommandSpec::new("cargo", ["package", "-p", "git-slop", "--locked"]),
        CommandSpec::new(
            "cargo",
            ["publish", "-p", "git-slop", "--dry-run", "--locked"],
        ),
    ]
}

pub fn prepare_release(options: &PrepareReleaseOptions) -> Result<PrepareReleaseResult> {
    prepare_release_with_runner(options, &mut SystemCommandRunner)
}

pub fn prepare_release_with_runner(
    options: &PrepareReleaseOptions,
    runner: &mut impl CommandRunner,
) -> Result<PrepareReleaseResult> {
    let state =
        validate_release_state_with_runner(&options.project_root, &options.version, runner)?;
    let commands = release_validation_commands();
    for command in &commands {
        runner.run(&options.project_root, command)?;
    }
    let messages = release_messages(&state);
    Ok(PrepareReleaseResult {
        state,
        commands,
        messages,
    })
}

pub fn release_messages(state: &ReleaseState) -> Vec<String> {
    vec![
        format!(
            "Validated release candidate {} at {} before tag creation.",
            state.tag, state.revision
        ),
        "Validated formatting, linting, tests, Cargo packaging, and crates.io dry-run.".to_owned(),
        "Release preparation performed no publication and wrote no Homebrew formula.".to_owned(),
        format!(
            "Dispatch the branch-restricted Release Publish workflow from exact main revision {}; that explicit dispatch authorizes crate publication, tag {}, draft release creation, and the immutable Homebrew receiver handoff.",
            state.revision, state.tag
        ),
        "Publish the verified draft only through the sole manual Marketplace approval with 2FA."
            .to_owned(),
    ]
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;
    use std::fs;

    use anyhow::anyhow;

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

    fn project() -> Result<tempfile::TempDir> {
        let temp = tempfile::tempdir()?;
        fs::write(
            temp.path().join("Cargo.toml"),
            "[package]\nname = \"git-slop\"\nversion = \"0.9.0\"\n",
        )?;
        Ok(temp)
    }

    #[test]
    fn release_candidate_requires_version_but_not_a_tag() -> Result<()> {
        let project = project()?;
        let revision = "a".repeat(40);
        let mut runner = FakeRunner {
            outputs: VecDeque::from([String::new(), revision.clone(), String::new()]),
            ..FakeRunner::default()
        };
        let state = validate_release_state_with_runner(project.path(), "0.9.0", &mut runner)?;
        assert_eq!(state.tag, "v0.9.0");
        assert_eq!(state.revision, revision);
        assert_eq!(runner.output_calls.len(), 3);
        assert_eq!(
            runner.output_calls[0].1,
            CommandSpec::new("git", ["status", "--porcelain=v1", "--untracked-files=all"])
        );
        assert_eq!(
            runner.output_calls[1].1,
            CommandSpec::new("git", ["rev-parse", "HEAD"])
        );
        assert_eq!(
            runner.output_calls[2].1,
            CommandSpec::new(
                "git",
                [
                    "for-each-ref".to_owned(),
                    "--format=%(*objectname)%09%(objectname)".to_owned(),
                    "refs/tags/v0.9.0".to_owned(),
                ]
            )
        );
        Ok(())
    }

    #[test]
    fn existing_release_tag_must_equal_candidate_head() -> Result<()> {
        let project = project()?;
        let revision = "a".repeat(40);
        let mut exact = FakeRunner {
            outputs: VecDeque::from([String::new(), revision.clone(), revision.clone()]),
            ..FakeRunner::default()
        };
        assert_eq!(
            validate_release_state_with_runner(project.path(), "0.9.0", &mut exact)?.revision,
            revision
        );

        let mut drifted = FakeRunner {
            outputs: VecDeque::from([String::new(), "a".repeat(40), "b".repeat(40)]),
            ..FakeRunner::default()
        };
        let error =
            validate_release_state_with_runner(project.path(), "0.9.0", &mut drifted).unwrap_err();
        assert!(error.to_string().contains("existing release tag v0.9.0"));
        Ok(())
    }

    #[test]
    fn release_candidate_rejects_dirty_or_untracked_paths_before_reading_head() -> Result<()> {
        let project = project()?;
        let mut runner = FakeRunner {
            outputs: VecDeque::from([" M Cargo.toml\n?? release-private.asc".to_owned()]),
            ..FakeRunner::default()
        };
        let error =
            validate_release_state_with_runner(project.path(), "0.9.0", &mut runner).unwrap_err();
        assert!(error.to_string().contains("worktree must be clean"));
        assert_eq!(runner.output_calls.len(), 1);
        Ok(())
    }

    #[test]
    fn release_preparation_is_validation_only() -> Result<()> {
        let project = project()?;
        let mut runner = FakeRunner {
            outputs: VecDeque::from([String::new(), "b".repeat(40), String::new()]),
            ..FakeRunner::default()
        };
        let options = PrepareReleaseOptions::new(project.path(), "0.9.0");
        let result = prepare_release_with_runner(&options, &mut runner)?;
        assert_eq!(runner.run_calls.len(), release_validation_commands().len());
        let rendered = result
            .commands
            .iter()
            .map(CommandSpec::display)
            .collect::<Vec<_>>()
            .join("\n");
        for forbidden in ["git tag", "git push", "gh ", "brew ", "homebrew-formula"] {
            assert!(!rendered.contains(forbidden));
        }
        assert!(rendered.contains("cargo publish -p git-slop --dry-run --locked"));
        assert!(!rendered.contains("cargo publish -p git-slop --locked"));
        assert!(
            result
                .messages
                .join("\n")
                .contains("no publication and wrote no Homebrew formula")
        );
        Ok(())
    }

    #[test]
    fn command_matrix_keeps_public_and_private_workspaces_explicit() {
        let commands = release_validation_commands()
            .iter()
            .map(CommandSpec::display)
            .collect::<Vec<_>>();
        assert_eq!(commands[0], "cargo fmt --all -- --check");
        assert_eq!(
            commands[1],
            "cargo fmt --manifest-path xtask/Cargo.toml --all -- --check"
        );
        assert_eq!(commands[6], "cargo package -p git-slop --locked");
        assert_eq!(commands[7], "cargo publish -p git-slop --dry-run --locked");
    }
}
