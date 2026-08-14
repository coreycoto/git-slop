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
    let mut paths = Vec::new();
    for raw in output
        .stdout
        .split(|byte| *byte == 0)
        .filter(|raw| !raw.is_empty())
    {
        let path = std::str::from_utf8(raw).with_context(|| {
            format!(
                "tracked path is not valid UTF-8 (hex {}); report JSON cannot represent it losslessly",
                hex::encode(raw)
            )
        })?;
        paths.push(path.replace('\\', "/"));
    }
    paths.sort();
    Ok(paths)
}

fn optional_git_output(repo_root: &Path, args: &[&str]) -> Option<String> {
    git_output(Some(repo_root), args)
        .ok()
        .filter(|value| !value.is_empty())
}

fn sanitize_remote_url(remote: String) -> Option<String> {
    let remote = remote
        .trim()
        .chars()
        .filter(|character| !character.is_control())
        .collect::<String>();
    let mut sanitized = remote
        .split(['?', '#'])
        .next()
        .unwrap_or_default()
        .to_string();
    let local_path = sanitized.starts_with("file://")
        || Path::new(&sanitized).is_absolute()
        || sanitized.starts_with("./")
        || sanitized.starts_with("../");
    if local_path {
        return Some(format!(
            "local:sha256:{}",
            hex::encode(Sha256::digest(sanitized.as_bytes()))
        ));
    }
    if let Some(scheme) = sanitized.find("://") {
        let authority = scheme + 3;
        if let Some(at) = sanitized[authority..].find('@') {
            sanitized.replace_range(authority..authority + at + 1, "");
        }
        return (!sanitized.is_empty()).then_some(sanitized);
    }
    // SCP-like remotes commonly include a user name before the host.
    if let (Some(at), Some(colon)) = (sanitized.find('@'), sanitized.find(':')) {
        if at < colon {
            sanitized.replace_range(..=at, "");
        }
    }
    (!sanitized.is_empty()).then_some(sanitized)
}

fn normalized_remote_identity(remote: &str) -> Option<String> {
    if remote.starts_with("local:sha256:") {
        return None;
    }
    let without_scheme = remote
        .split_once("://")
        .map_or(remote, |(_, remainder)| remainder);
    let normalized = if !remote.contains("://") {
        if let Some((host, path)) = without_scheme.split_once(':') {
            format!("{host}/{path}")
        } else {
            without_scheme.to_string()
        }
    } else {
        without_scheme.to_string()
    };
    let mut parts = normalized.trim_matches('/').split('/');
    let host = parts.next()?.to_ascii_lowercase();
    let owner = parts.next()?.to_ascii_lowercase();
    let name = parts.next()?.trim_end_matches(".git").to_ascii_lowercase();
    if host.is_empty() || owner.is_empty() || name.is_empty() {
        return None;
    }
    Some(format!("remote:{host}/{owner}/{name}"))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorktreeState {
    pub clean: bool,
    pub staged_change_count: usize,
    pub modified_tracked_file_count: usize,
    pub untracked_file_count: usize,
    pub digest: String,
}

fn porcelain_counts(raw: &[u8]) -> (usize, usize, usize) {
    let mut staged = 0;
    let mut modified = 0;
    let mut untracked = 0;
    let entries = raw.split(|byte| *byte == 0).collect::<Vec<_>>();
    let mut index = 0;
    while index < entries.len() {
        let entry = entries[index];
        index += 1;
        if entry.len() < 3 {
            continue;
        }
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
        if matches!(entry[0], b'R' | b'C') || matches!(entry[1], b'R' | b'C') {
            index = index.saturating_add(1);
        }
    }
    (staged, modified, untracked)
}

pub fn worktree_state(repo_root: &Path) -> Result<WorktreeState> {
    worktree_state_excluding(repo_root, &[])
}

pub fn worktree_state_excluding(
    repo_root: &Path,
    excluded_roots: &[String],
) -> Result<WorktreeState> {
    let mut arguments = vec![
        "status".to_string(),
        "--porcelain=v1".to_string(),
        "-z".to_string(),
        "--untracked-files=all".to_string(),
    ];
    if !excluded_roots.is_empty() {
        arguments.extend(["--".to_string(), ".".to_string()]);
        for path in excluded_roots {
            let path = path.trim_matches('/');
            arguments.push(format!(":(exclude,top){path}"));
            arguments.push(format!(":(exclude,top){path}/**"));
        }
    }
    let output = Command::new("git")
        .current_dir(repo_root)
        .args(&arguments)
        .output()
        .context("failed to inspect Git worktree state")?;
    if !output.status.success() {
        bail!(
            "git status failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    let (staged, modified, untracked) = porcelain_counts(&output.stdout);
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
    let symbolic_branch =
        optional_git_output(repo_root, &["symbolic-ref", "--short", "-q", "HEAD"]);
    let head_commit = optional_git_output(repo_root, &["rev-parse", "HEAD"]);
    let detached_head = symbolic_branch.is_none() && head_commit.is_some();
    let branch = symbolic_branch.or_else(|| {
        optional_git_output(repo_root, &["describe", "--tags", "--exact-match", "HEAD"])
    });
    let head_commit_timestamp = head_commit
        .as_ref()
        .and_then(|_| optional_git_output(repo_root, &["show", "-s", "--format=%cI", "HEAD"]));
    let git_remote_url = optional_git_output(repo_root, &["config", "--get", "remote.origin.url"])
        .and_then(sanitize_remote_url);
    let remote_identity = git_remote_url
        .as_deref()
        .and_then(normalized_remote_identity);
    let root_commit = optional_git_output(repo_root, &["rev-list", "--max-parents=0", "HEAD"])
        .and_then(|roots| roots.lines().min().map(ToOwned::to_owned));
    let (repository_id, repository_identity_source) = if let Some(identity) = remote_identity {
        (Some(identity), Some("normalized_remote".to_string()))
    } else if let Some(root) = root_commit {
        (
            Some(format!("root:{root}")),
            Some("root_commit".to_string()),
        )
    } else {
        (None, None)
    };
    let is_shallow = optional_git_output(repo_root, &["rev-parse", "--is-shallow-repository"])
        .is_some_and(|value| value == "true");
    let worktree = worktree_state(repo_root)?;
    Ok(RepoMetadata {
        repo_name,
        repo_root: canonical.to_string_lossy().into_owned(),
        repository_id,
        repository_identity_source,
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

#[cfg(test)]
mod tests {
    use super::{normalized_remote_identity, porcelain_counts, sanitize_remote_url};

    #[test]
    fn porcelain_rename_second_paths_are_not_counted_as_changes() {
        assert_eq!(
            porcelain_counts(b"R  new name.rs\0old name.rs\0 M tracked.rs\0?? new.rs\0"),
            (1, 1, 1)
        );
    }

    #[test]
    fn provenance_remote_sanitization_removes_secrets_and_fragments() {
        assert_eq!(
            sanitize_remote_url(
                "https://user:token@example.com/owner/repo.git?secret=yes#x".to_string()
            ),
            Some("https://example.com/owner/repo.git".to_string())
        );
        assert_eq!(
            sanitize_remote_url("token@example.com:owner/repo.git".to_string()),
            Some("example.com:owner/repo.git".to_string())
        );
        assert!(
            sanitize_remote_url("file:///Users/person/private/repo".to_string())
                .is_some_and(|value| value.starts_with("local:sha256:"))
        );
    }

    #[test]
    fn repository_identity_normalizes_transport_and_case() {
        assert_eq!(
            normalized_remote_identity("https://GitHub.com/CoreyCoto/git-slop.git"),
            Some("remote:github.com/coreycoto/git-slop".to_string())
        );
        assert_eq!(
            normalized_remote_identity("github.com:CoreyCoto/git-slop.git"),
            Some("remote:github.com/coreycoto/git-slop".to_string())
        );
        assert_eq!(normalized_remote_identity("local:sha256:abc"), None);
    }
}
