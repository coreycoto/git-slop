use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, anyhow, bail};
use chrono::{DateTime, Utc};

use crate::{analyze, config};

pub(crate) struct MaterializedBaseline {
    repo_root: PathBuf,
    temporary_root: PathBuf,
    worktree: PathBuf,
    pub report_path: PathBuf,
    pub revision: String,
    pub copied_head_config: bool,
}

fn safe_revision(reference: &str) -> bool {
    let mut base_ended = false;
    let mut previous_operator = false;
    if reference.is_empty() || reference.len() > 200 || reference.contains("..") {
        return false;
    }
    for (index, character) in reference.chars().enumerate() {
        if character == '~' || character == '^' {
            if index == 0 || previous_operator {
                return false;
            }
            base_ended = true;
            previous_operator = true;
        } else if base_ended {
            if !character.is_ascii_digit() {
                return false;
            }
            previous_operator = false;
        } else if !(character.is_ascii_alphanumeric() || matches!(character, '.' | '_' | '/' | '-'))
        {
            return false;
        }
    }
    true
}

fn git_output(repo_root: &Path, arguments: &[&str]) -> Result<String> {
    let output = Command::new("git")
        .current_dir(repo_root)
        .args(arguments)
        .output()
        .context("failed to execute git")?;
    if !output.status.success() {
        bail!(
            "git {} failed: {}",
            arguments.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

impl MaterializedBaseline {
    pub(crate) fn create(
        repo_root: &Path,
        reference: &str,
        scope: Option<String>,
        allow_shallow: bool,
        as_of: Option<DateTime<Utc>>,
    ) -> Result<Self> {
        if !safe_revision(reference) {
            bail!("baseline reference is not a bounded safe Git revision: {reference:?}");
        }
        let revision = git_output(
            repo_root,
            &[
                "rev-parse",
                "--verify",
                "--end-of-options",
                &format!("{reference}^{{commit}}"),
            ],
        )?;
        if revision.len() != 40 || !revision.chars().all(|value| value.is_ascii_hexdigit()) {
            bail!("baseline reference did not resolve to a full commit SHA");
        }
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| anyhow!("system clock predates the Unix epoch"))?
            .as_nanos();
        let temporary_root =
            std::env::temp_dir().join(format!("git-slop-baseline-{}-{nonce}", std::process::id()));
        let worktree = temporary_root.join("worktree");
        fs::create_dir_all(&temporary_root)?;
        let add = Command::new("git")
            .current_dir(repo_root)
            .args(["worktree", "add", "--detach"])
            .arg(&worktree)
            .arg(&revision)
            .output()?;
        if !add.status.success() {
            let _ = fs::remove_dir_all(&temporary_root);
            bail!(
                "could not create isolated baseline worktree: {}",
                String::from_utf8_lossy(&add.stderr).trim()
            );
        }

        let mut materialized = Self {
            repo_root: repo_root.to_path_buf(),
            temporary_root,
            report_path: worktree.join(".slop/latest/report.json"),
            worktree,
            revision,
            copied_head_config: false,
        };
        let head_config = config::config_path(repo_root);
        if head_config.is_file() {
            let baseline_config = config::config_path(&materialized.worktree);
            if let Some(parent) = baseline_config.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::copy(&head_config, &baseline_config)?;
            materialized.copied_head_config = true;
        }
        let result = analyze::run_find_with_options(
            &materialized.worktree,
            &analyze::FindOptions {
                allow_shallow,
                scope,
                progress: false,
                no_cache: true,
                as_of,
                ..analyze::FindOptions::default()
            },
        );
        if let Err(error) = result {
            drop(materialized);
            return Err(error).context("isolated baseline scan failed");
        }
        Ok(materialized)
    }
}

impl Drop for MaterializedBaseline {
    fn drop(&mut self) {
        let _ = Command::new("git")
            .current_dir(&self.repo_root)
            .args(["worktree", "remove", "--force"])
            .arg(&self.worktree)
            .output();
        let _ = fs::remove_dir_all(&self.temporary_root);
    }
}

#[cfg(test)]
mod tests {
    use super::safe_revision;

    #[test]
    fn bounded_revision_syntax_accepts_ancestry_without_reflog_or_ranges() {
        for valid in [
            "HEAD",
            "HEAD^",
            "HEAD^2",
            "main~12",
            "refs/tags/v0.11.0",
            "abc123",
        ] {
            assert!(safe_revision(valid), "{valid}");
        }
        for invalid in [
            "",
            "HEAD@{1}",
            "HEAD..main",
            "^HEAD",
            "HEAD^^",
            "HEAD;touch-x",
        ] {
            assert!(!safe_revision(invalid), "{invalid}");
        }
    }
}
