use std::path::{Path, PathBuf};

use anyhow::{Result, bail};

use crate::homebrew::write_formula;
use crate::manifest::{
    CommandRunner, CommandSpec, ReleaseIdentity, SystemCommandRunner, git_revision_with_runner,
    is_strict_semver, project_version, resolve_project_path,
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
    pub tap: PathBuf,
}

impl PrepareReleaseOptions {
    pub fn new(
        project_root: impl Into<PathBuf>,
        version: impl Into<String>,
        tap: impl Into<PathBuf>,
    ) -> Self {
        Self {
            project_root: project_root.into(),
            version: version.into(),
            tap: tap.into(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PrepareReleaseResult {
    pub state: ReleaseState,
    pub formula_path: PathBuf,
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

pub fn tag_revision(project_root: &Path, tag: &str) -> Result<String> {
    tag_revision_with_runner(project_root, tag, &mut SystemCommandRunner)
}

pub fn tag_revision_with_runner(
    project_root: &Path,
    tag: &str,
    runner: &mut impl CommandRunner,
) -> Result<String> {
    git_revision_with_runner(project_root, tag, runner)
        .map_err(|error| anyhow::anyhow!("release tag {tag} does not exist: {error}"))
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

pub fn validate_release_state(project_root: &Path, version: &str) -> Result<ReleaseState> {
    validate_release_state_with_runner(project_root, version, &mut SystemCommandRunner)
}

pub fn validate_release_state_with_runner(
    project_root: &Path,
    version: &str,
    runner: &mut impl CommandRunner,
) -> Result<ReleaseState> {
    validate_project_version(project_root, version)?;
    let tag = format!("v{version}");
    let revision = tag_revision_with_runner(project_root, &tag, runner)?;
    let head = head_revision_with_runner(project_root, runner)?;
    if revision != head {
        bail!(
            "release tag {tag} resolves to {revision}, but HEAD is {head}; prepare and publish \
             from the exact tagged commit."
        );
    }
    Ok(ReleaseState { tag, revision })
}

/// Commands run by release preparation, in execution order.
///
/// The public crate and private xtask are deliberately separate workspaces, so
/// each gate names its intended manifest or package. `cargo fmt` has no locked
/// mode; all dependency-resolving private gates use the xtask lockfile.
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

    let relative_formula = options.tap.join("Formula/git-slop.rb");
    let formula_path = resolve_project_path(&options.project_root, &relative_formula)?;
    let identity =
        ReleaseIdentity::new(&options.version, state.tag.clone(), state.revision.clone());
    write_formula(&options.project_root, &formula_path, &identity)?;

    let messages = release_messages(&state, &formula_path);
    Ok(PrepareReleaseResult {
        state,
        formula_path,
        commands,
        messages,
    })
}

pub fn release_messages(state: &ReleaseState, formula_path: &Path) -> Vec<String> {
    let tap_root = formula_path
        .parent()
        .and_then(Path::parent)
        .unwrap_or(formula_path);
    vec![
        format!("Verified local tag {} at {}.", state.tag, state.revision),
        "Validated formatting, linting, tests, Cargo packaging, and crates.io dry-run.".to_owned(),
        format!(
            "Prepared native Rust Homebrew formula: {}",
            formula_path.display()
        ),
        format!("Push release tag: git push origin {}", state.tag),
        "Watch release workflow: gh run list --repo coreycoto/git-slop --workflow \
         release-publish.yml --limit 1"
            .to_owned(),
        format!(
            "Verify GitHub Release assets: gh release view {} --repo coreycoto/git-slop --json \
             url,tagName,assets",
            state.tag
        ),
        format!(
            "Verify tap formula: cd {} && brew style Formula/git-slop.rb",
            tap_root.display()
        ),
        "Upgrade lane (install the prior tap formula before merging the tap update): brew update \
         && brew upgrade coreycoto/tap/git-slop"
            .to_owned(),
        "Clean-install lane (use a separate clean runner): brew install \
         coreycoto/tap/git-slop"
            .to_owned(),
        "Test both lanes: brew test coreycoto/tap/git-slop".to_owned(),
        "Confirm CLI: git-slop version".to_owned(),
        "Confirm Git command: git slop --help".to_owned(),
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
    fn project_version_requires_strict_matching_semver() -> Result<()> {
        let project = project()?;
        validate_project_version(project.path(), "0.9.0")?;

        let mismatch = validate_project_version(project.path(), "9.9.9").unwrap_err();
        assert!(mismatch.to_string().contains("Cargo.toml version"));
        let leading_zero = validate_project_version(project.path(), "00.9.0").unwrap_err();
        assert!(leading_zero.to_string().contains("strict semver"));
        let prerelease = validate_project_version(project.path(), "0.9.0-rc.1").unwrap_err();
        assert!(prerelease.to_string().contains("strict semver"));
        Ok(())
    }

    #[test]
    fn release_state_requires_exact_tagged_head() -> Result<()> {
        let project = project()?;
        let mut runner = FakeRunner {
            outputs: VecDeque::from(["a".repeat(40), "b".repeat(40)]),
            ..FakeRunner::default()
        };
        let error =
            validate_release_state_with_runner(project.path(), "0.9.0", &mut runner).unwrap_err();
        assert!(error.to_string().contains("exact tagged commit"));
        assert_eq!(
            runner.output_calls[0].1,
            CommandSpec::new(
                "git",
                ["rev-parse", "--verify", "refs/tags/v0.9.0^{commit}",]
            )
        );
        assert_eq!(
            runner.output_calls[1].1,
            CommandSpec::new("git", ["rev-parse", "HEAD"])
        );
        Ok(())
    }

    #[test]
    fn release_preparation_runs_only_validation_and_dry_run_commands() -> Result<()> {
        let project = project()?;
        let mut runner = FakeRunner {
            outputs: VecDeque::from(["c".repeat(40), "c".repeat(40)]),
            ..FakeRunner::default()
        };
        let options = PrepareReleaseOptions::new(project.path(), "0.9.0", "tap");
        let result = prepare_release_with_runner(&options, &mut runner)?;

        assert_eq!(runner.run_calls.len(), release_validation_commands().len());
        assert!(
            runner
                .run_calls
                .iter()
                .all(|(cwd, _)| cwd == project.path())
        );
        assert_eq!(
            runner
                .run_calls
                .iter()
                .map(|(_, command)| command.clone())
                .collect::<Vec<_>>(),
            release_validation_commands()
        );
        let rendered_commands = result
            .commands
            .iter()
            .map(CommandSpec::display)
            .collect::<Vec<_>>()
            .join("\n");
        for forbidden in ["git tag", "git push", "gh ", "brew "] {
            assert!(!rendered_commands.contains(forbidden));
        }
        assert!(rendered_commands.contains("cargo publish -p git-slop --dry-run --locked"));
        assert!(!rendered_commands.contains("cargo publish -p git-slop --locked"));
        assert!(
            rendered_commands
                .contains("cargo clippy --manifest-path xtask/Cargo.toml --all-targets")
        );

        let formula = fs::read_to_string(&result.formula_path)?;
        assert!(formula.contains("git-slop 0.9.0"));
        let messages = result.messages.join("\n");
        assert!(messages.contains("git push origin v0.9.0"));
        assert!(messages.contains("brew upgrade coreycoto/tap/git-slop"));
        assert!(messages.contains("separate clean runner"));
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
        assert!(commands[2].starts_with("cargo clippy -p git-slop "));
        assert!(commands[3].starts_with("cargo clippy --manifest-path xtask/Cargo.toml "));
        assert!(commands[4].starts_with("cargo test -p git-slop "));
        assert!(commands[5].starts_with("cargo test --manifest-path xtask/Cargo.toml "));
        assert_eq!(commands[6], "cargo package -p git-slop --locked");
        assert_eq!(commands[7], "cargo publish -p git-slop --dry-run --locked");
    }
}
