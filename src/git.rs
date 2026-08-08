use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};
use sha2::{Digest, Sha256};

use crate::model::RepoMetadata;

fn git_output(repo_root: Option<&Path>, args: &[&str]) -> Result<String> {
    let mut command = Command::new("git");
    if let Some(root) = repo_root {
        command.current_dir(root);
    }
    let output = command
        .args(args)
        .output()
        .context("failed to execute git")?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        bail!(
            "git {} failed{}",
            args.join(" "),
            if stderr.is_empty() {
                String::new()
            } else {
                format!(": {stderr}")
            }
        );
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

pub fn resolve_repo_root() -> Result<PathBuf> {
    resolve_repo_root_from(None)
}

pub fn resolve_repo_root_from(start: Option<&Path>) -> Result<PathBuf> {
    let root = git_output(start, &["rev-parse", "--show-toplevel"])?;
    Ok(PathBuf::from(root))
}

pub fn list_tracked_files(repo_root: &Path) -> Result<Vec<String>> {
    let output = Command::new("git")
        .current_dir(repo_root)
        .args(["ls-files", "-z"])
        .output()
        .context("failed to list tracked files")?;
    if !output.status.success() {
        bail!(
            "git ls-files failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    let mut paths: Vec<String> = output
        .stdout
        .split(|byte| *byte == 0)
        .filter(|raw| !raw.is_empty())
        .map(|raw| String::from_utf8_lossy(raw).replace('\\', "/"))
        .collect();
    paths.sort();
    Ok(paths)
}

fn optional_git_output(repo_root: &Path, args: &[&str]) -> Option<String> {
    git_output(Some(repo_root), args)
        .ok()
        .filter(|value| !value.is_empty())
}

fn sanitize_remote_url(remote: String) -> String {
    if let Some(scheme) = remote.find("://") {
        let authority = scheme + 3;
        if let Some(at) = remote[authority..].find('@') {
            let mut sanitized = remote.clone();
            sanitized.replace_range(authority..authority + at + 1, "");
            return sanitized;
        }
    }
    remote
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorktreeState {
    pub clean: bool,
    pub staged_change_count: usize,
    pub modified_tracked_file_count: usize,
    pub untracked_file_count: usize,
    pub digest: String,
}

pub fn worktree_state(repo_root: &Path) -> Result<WorktreeState> {
    let output = Command::new("git")
        .current_dir(repo_root)
        .args(["status", "--porcelain=v1", "-z", "--untracked-files=all"])
        .output()
        .context("failed to inspect Git worktree state")?;
    if !output.status.success() {
        bail!(
            "git status failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    let mut staged = 0;
    let mut modified = 0;
    let mut untracked = 0;
    for entry in output
        .stdout
        .split(|byte| *byte == 0)
        .filter(|entry| entry.len() >= 3)
    {
        if entry.starts_with(b"?? ") {
            untracked += 1;
            continue;
        }
        if entry[0] != b' ' && entry[0] != b'?' {
            staged += 1;
        }
        if entry[1] != b' ' && entry[1] != b'?' {
            modified += 1;
        }
    }
    Ok(WorktreeState {
        clean: output.stdout.is_empty(),
        staged_change_count: staged,
        modified_tracked_file_count: modified,
        untracked_file_count: untracked,
        digest: hex::encode(Sha256::digest(&output.stdout)),
    })
}

pub fn repo_metadata(repo_root: &Path) -> Result<RepoMetadata> {
    let canonical = repo_root
        .canonicalize()
        .unwrap_or_else(|_| repo_root.to_path_buf());
    let repo_name = canonical
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("repository")
        .to_string();
    let branch = optional_git_output(repo_root, &["symbolic-ref", "--short", "-q", "HEAD"]);
    let head_commit = optional_git_output(repo_root, &["rev-parse", "HEAD"]);
    let head_commit_timestamp = head_commit
        .as_ref()
        .and_then(|_| optional_git_output(repo_root, &["show", "-s", "--format=%cI", "HEAD"]));
    let git_remote_url = optional_git_output(repo_root, &["config", "--get", "remote.origin.url"])
        .map(sanitize_remote_url);
    let is_shallow = optional_git_output(repo_root, &["rev-parse", "--is-shallow-repository"])
        .is_some_and(|value| value == "true");
    let worktree = worktree_state(repo_root)?;
    let detached_head = branch.is_none() && head_commit.is_some();
    Ok(RepoMetadata {
        repo_name,
        repo_root: canonical.to_string_lossy().into_owned(),
        branch,
        head_commit,
        head_commit_timestamp,
        git_remote_url,
        is_shallow,
        detached_head,
        worktree_clean: worktree.clean,
        staged_change_count: worktree.staged_change_count,
        modified_tracked_file_count: worktree.modified_tracked_file_count,
        untracked_file_count: worktree.untracked_file_count,
        worktree_state_digest: worktree.digest,
        analyzed_content_digest: None,
    })
}

pub fn changed_files(repo_root: &Path, base: &str, head: &str) -> Result<Vec<String>> {
    let output = git_output(
        Some(repo_root),
        &["diff", "--name-only", "--diff-filter=ACMR", base, head],
    )?;
    let mut paths: Vec<String> = output
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(ToOwned::to_owned)
        .collect();
    paths.sort();
    paths.dedup();
    Ok(paths)
}
