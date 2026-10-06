use anyhow::{Context, Result, ensure};
use serde_json::Value;
use std::{
    fs,
    io::Write,
    path::Path,
    process::{Command, Output, Stdio},
};

use super::super::paths::validate_result_paths;
use super::common::*;

pub(super) fn recompute_commit(
    patch: &Path,
    source_sha: &str,
    expected_patch_sha: &str,
    expected_files: &Value,
    created_at: &str,
    title: &str,
) -> Result<String> {
    validate_sha40(source_sha)?;
    ensure!(
        sha256(&fs::read(patch)?) == expected_patch_sha,
        "publication patch digest mismatch"
    );
    git_call(&["fetch", "--no-tags", "origin", source_sha])
        .context("exact publication source commit could not be fetched")?;
    git_call(&["checkout", "--detach", source_sha])
        .context("could not select the exact publication source commit")?;
    git_call(&[
        "-c",
        "core.hooksPath=/dev/null",
        "apply",
        "--cached",
        "--check",
        "--binary",
        path_text(patch)?,
    ])
    .context("captured patch no longer applies to its exact source commit")?;
    git_call(&[
        "-c",
        "core.hooksPath=/dev/null",
        "apply",
        "--cached",
        "--binary",
        path_text(patch)?,
    ])
    .context("captured patch could not be applied to the index")?;
    let files = git_output(&["diff", "--cached", "--name-only", "-z", source_sha])?;
    let actual = files
        .split(|byte| *byte == 0)
        .filter(|part| !part.is_empty())
        .map(|part| String::from_utf8(part.to_vec()))
        .collect::<std::result::Result<Vec<_>, _>>()
        .context("changed file names are not valid UTF-8")?;
    let mut actual_sorted = actual;
    actual_sorted.sort();
    ensure!(
        Value::Array(actual_sorted.iter().cloned().map(Value::String).collect()) == *expected_files,
        "patch paths differ from the captured dependency remediation plan"
    );
    let tree = git_out(Path::new("."), &["write-tree"])?;
    let message = format!("{title}\n\nPrepared by the trusted dependency remediation workflow.\n");
    let mut command = git_command(&[
        "-c",
        "core.hooksPath=/dev/null",
        "commit-tree",
        &tree,
        "-p",
        source_sha,
    ])?;
    command
        .env("GIT_AUTHOR_NAME", "github-actions[bot]")
        .env(
            "GIT_AUTHOR_EMAIL",
            "41898282+github-actions[bot]@users.noreply.github.com",
        )
        .env("GIT_COMMITTER_NAME", "github-actions[bot]")
        .env(
            "GIT_COMMITTER_EMAIL",
            "41898282+github-actions[bot]@users.noreply.github.com",
        )
        .env("GIT_AUTHOR_DATE", created_at)
        .env("GIT_COMMITTER_DATE", created_at)
        .stdin(Stdio::piped());
    let mut child = command.spawn()?;
    child
        .stdin
        .take()
        .context("git commit-tree stdin is unavailable")?
        .write_all(message.as_bytes())?;
    let output = child.wait_with_output()?;
    ensure!(
        output.status.success(),
        "git could not construct the exact commit"
    );
    let commit = String::from_utf8(output.stdout)?.trim().to_owned();
    validate_sha40(&commit)?;
    Ok(commit)
}

pub(super) fn sorted_result_files(result: &Value) -> Result<Value> {
    let mut paths = validate_result_paths(result)?;
    paths.sort();
    Ok(Value::Array(paths.into_iter().map(Value::String).collect()))
}

pub(super) fn remote_ref_sha(ref_name: &str) -> Result<String> {
    let output = git_output(&["ls-remote", "--refs", "origin", ref_name])?;
    let text = String::from_utf8(output)?;
    if text.is_empty() {
        return Ok(String::new());
    }
    let lines: Vec<_> = text.lines().collect();
    ensure!(
        lines.len() == 1,
        "remote ref read returned duplicate identities"
    );
    let mut fields = lines[0].split_whitespace();
    let sha = fields.next().unwrap_or_default();
    let actual_ref = fields.next().unwrap_or_default();
    ensure!(
        fields.next().is_none() && actual_ref == ref_name,
        "remote ref read returned a malformed identity"
    );
    validate_sha40(sha)?;
    Ok(sha.to_owned())
}

pub(super) fn normalize_remote(url: &str) -> Result<()> {
    let server_url = required_env("GITHUB_SERVER_URL")?;
    let host = server_url
        .trim_start_matches("https://")
        .trim_end_matches('/');
    let repo = required_env("GITHUB_REPOSITORY")?;
    let exact = [
        format!("https://{host}/{repo}"),
        format!("https://{host}/{repo}.git"),
        format!("http://{host}/{repo}"),
        format!("http://{host}/{repo}.git"),
        format!("git@{host}:{repo}"),
        format!("git@{host}:{repo}.git"),
        format!("ssh://git@{host}/{repo}"),
        format!("ssh://git@{host}/{repo}.git"),
    ];
    ensure!(
        exact.iter().any(|candidate| candidate == url),
        "git origin does not identify the exact workflow repository"
    );
    Ok(())
}

pub(super) fn validate_git_refs(refs: &[&str]) -> Result<()> {
    for value in refs {
        ensure!(
            !value.is_empty() && !value.contains(['\n', '\r']),
            "a saved Git ref is empty or malformed"
        );
        git_call(&["check-ref-format", value])
            .with_context(|| format!("Git rejected saved ref {value}"))?;
    }
    Ok(())
}

pub(super) fn canonical_sha(root: &Path, file: &Path) -> Result<String> {
    require_regular(file, "canonical digest input is missing or unsafe")?;
    let repo = format!(
        "{}/{}",
        required_env("GITHUB_SERVER_URL")?.trim_end_matches('/'),
        required_env("GITHUB_REPOSITORY")?
    );
    let input = format!("document={}", file.display());
    let steward = required_env("GH_STEWARD_BIN")?;
    let output = Command::new(steward)
        .args(["runs", "digest", "--repo-root"])
        .arg(root)
        .args(["--repo", &repo, "--input", &input, "--format", "json"])
        .output()
        .context("gh-steward could not compute the canonical document digest")?;
    ensure!(
        output.status.success(),
        "gh-steward could not compute the canonical document digest"
    );
    let envelope: Value = serde_json::from_slice(&output.stdout)
        .context("gh-steward returned malformed canonical document digest JSON")?;
    ensure!(
        envelope["schema_version"] == 2,
        "gh-steward returned a malformed canonical document digest"
    );
    let digest = value_string(&envelope["data"], "sha256")?;
    ensure!(
        is_sha256(digest),
        "gh-steward returned a malformed canonical document digest"
    );
    Ok(digest.to_owned())
}

pub(super) fn git_status() -> Result<Vec<u8>> {
    git_output(&["status", "--porcelain"])
}
pub(super) fn git_out(root: &Path, args: &[&str]) -> Result<String> {
    String::from_utf8(git_output_at(root, args)?)
        .map(|s| s.trim_end().to_owned())
        .context("git returned non-UTF-8 output")
}
pub(super) fn git_call(args: &[&str]) -> Result<()> {
    let output = git_output(args)?;
    let _ = output;
    Ok(())
}
pub(super) fn git_output(args: &[&str]) -> Result<Vec<u8>> {
    git_output_at(Path::new("."), args)
}
pub(super) fn git_output_at(root: &Path, args: &[&str]) -> Result<Vec<u8>> {
    let output = git_command(args)?
        .current_dir(root)
        .output()
        .context("git command could not be started")?;
    ensure!(output.status.success(), "git command failed");
    Ok(output.stdout)
}
pub(super) fn git_command(args: &[&str]) -> Result<Command> {
    let mut command = Command::new("git");
    command
        .args(args)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_SYSTEM", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1");
    Ok(command)
}
pub(super) fn gh_output(args: &[&str]) -> Result<Output> {
    Command::new("gh")
        .args(args)
        .output()
        .context("gh command could not be started")
}
pub(super) fn gh_json(args: &[&str]) -> Result<Value> {
    let output = gh_output(args)?;
    ensure!(output.status.success(), "GitHub state read failed");
    serde_json::from_slice(&output.stdout).context("GitHub returned malformed JSON")
}
pub(super) fn gh_text(args: &[&str]) -> Result<String> {
    let output = gh_output(args)?;
    ensure!(output.status.success(), "GitHub state read failed");
    Ok(String::from_utf8(output.stdout)?.trim().to_owned())
}
