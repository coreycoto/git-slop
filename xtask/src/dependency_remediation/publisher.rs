mod common;
mod git;
mod pr;
mod prepare;
mod proof;
mod push;

use anyhow::{Context, Result, bail, ensure};
use std::path::{Path, PathBuf};

use common::{ensure_real_dir, read_json, required_env, string_at};
use git::git_out as trusted_git_out;
use pr::{create_pr, verify_pr};
use prepare::prepare;
use proof::{read_publication, safe_repo_identity, validate_context};
use push::push;

pub(super) struct Package {
    pub(super) root: PathBuf,
    pub(super) context: PathBuf,
    pub(super) intent: PathBuf,
    pub(super) patch: PathBuf,
    pub(super) result: PathBuf,
    pub(super) event: PathBuf,
    pub(super) candidate: PathBuf,
}

impl Package {
    pub(super) fn new(path: &Path) -> Result<Self> {
        ensure!(path.is_absolute(), "package must be an absolute directory");
        let metadata =
            std::fs::symlink_metadata(path).context("publication package is unavailable")?;
        ensure!(
            metadata.is_dir() && !metadata.file_type().is_symlink(),
            "package must be a non-symlink directory"
        );
        let root = std::fs::canonicalize(path)?;
        let package = Self {
            context: root.join("run-context.json"),
            intent: root.join("publication/intent.json"),
            patch: root.join("publication/patch.diff"),
            result: root.join("publication/result.json"),
            event: root.join("events/trigger-event.json"),
            candidate: root.join("publication/candidate.json"),
            root,
        };
        ensure_real_dir(&package.root.join("publication"))?;
        ensure_real_dir(&package.root.join("events"))?;
        Ok(package)
    }
}

pub(super) fn run(trusted_root: &Path, action: &str, _package: &Path) -> Result<()> {
    let trusted_root =
        std::fs::canonicalize(trusted_root).context("trusted control checkout is unavailable")?;
    reject_deferred_publication(&trusted_root, action)
}

fn reject_deferred_publication(trusted_root: &Path, action: &str) -> Result<()> {
    let policy = read_json(&trusted_root.join(".agents/gh-steward-recovery-policy.json"))?;
    let allow_publication =
        policy["workflows"]["dependency-remediation.yml"]["allow_publication"].as_bool();
    ensure!(
        allow_publication == Some(false),
        "dependency-remediation policy must explicitly keep publication disabled"
    );
    bail!(
        "dependency-remediation {action} is deferred: the pinned gh-steward release does not yet provide an authoritative GitHub publication plan, apply, and receipt"
    )
}

#[allow(
    dead_code,
    reason = "the Rust publisher stays unreachable until gh-steward owns GitHub mutation authority"
)]
fn run_enabled(trusted_root: &Path, action: &str, package: &Path) -> Result<()> {
    let package = Package::new(package)?;
    let trusted_root =
        std::fs::canonicalize(trusted_root).context("trusted control checkout is unavailable")?;
    let expected_sha = required_env("GITHUB_WORKFLOW_SHA")?;
    ensure!(
        trusted_git_out(&trusted_root, &["rev-parse", "HEAD"])? == expected_sha,
        "trusted control checkout differs from the workflow source",
    );
    for path in [&package.context, &package.result, &package.event] {
        common::require_regular(path, "required trusted package file is missing or unsafe")?;
    }
    safe_repo_identity()?;
    validate_context(&package)?;

    match action {
        "prepare" => prepare(&trusted_root, &package),
        "push" => push(&trusted_root, &package),
        "create-pr" => create_pr(&trusted_root, &package),
        "verify-pr" => verify_pr(&trusted_root, &package),
        "continue" => continue_publication(&trusted_root, &package),
        _ => bail!("unsupported dependency-remediation publisher action"),
    }
}

fn continue_publication(root: &Path, package: &Package) -> Result<()> {
    if !package.intent.exists() {
        prepare(root, package)?;
    }
    loop {
        read_publication(root, package)?;
        let current_context = read_json(&package.context)?;
        let stage = string_at(&current_context, &["publication", "stage"])?;
        match stage {
            "push-pending" => push(root, package)?,
            "pr-pending" => create_pr(root, package)?,
            "pr-verify-pending" => verify_pr(root, package)?,
            "completed" => return Ok(()),
            _ => bail!("saved publication has an unsupported or ambiguous continuation stage"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::run;
    use serde_json::json;
    use std::fs;
    use tempfile::TempDir;

    fn write_policy(root: &std::path::Path, allow_publication: bool) {
        let path = root.join(".agents/gh-steward-recovery-policy.json");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(
            path,
            serde_json::to_vec(&json!({
                "workflows": {
                    "dependency-remediation.yml": {
                        "allow_publication": allow_publication
                    }
                }
            }))
            .unwrap(),
        )
        .unwrap();
    }

    #[test]
    fn fresh_prepare_and_recovered_continue_fail_before_touching_packages() {
        let temp = TempDir::new().unwrap();
        write_policy(temp.path(), false);
        let package = temp.path().join("not-created");

        for action in ["prepare", "continue"] {
            let error = run(temp.path(), action, &package).unwrap_err();
            assert!(error.to_string().contains("is deferred"), "{error:#}");
            assert!(!package.exists(), "{action} touched the package");
        }
    }

    #[test]
    fn publication_cannot_be_reenabled_without_native_gh_steward_authority() {
        let temp = TempDir::new().unwrap();
        write_policy(temp.path(), true);
        let package = temp.path().join("not-created");

        let error = run(temp.path(), "continue", &package).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("must explicitly keep publication disabled"),
            "{error:#}"
        );
        assert!(!package.exists());
    }
}
