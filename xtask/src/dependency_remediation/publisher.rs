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

pub(super) fn run(trusted_root: &Path, action: &str, package: &Path) -> Result<()> {
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
