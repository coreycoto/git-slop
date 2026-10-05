use anyhow::{Context, Result, bail, ensure};
use serde_json::{Value, json};
use std::{
    env, fs,
    io::Write,
    path::{Path, PathBuf},
    process::Command,
};

use super::paths::read_result;

pub fn capture() -> Result<()> {
    let runner_temp = required_env("RUNNER_TEMP")?;
    let output_file = required_env("GITHUB_OUTPUT")?;
    let event_path = PathBuf::from(required_env("GITHUB_EVENT_PATH")?);
    let event_bytes = fs::read(&event_path).context("GitHub event payload is unavailable")?;
    let event: Value =
        serde_json::from_slice(&event_bytes).context("GitHub event payload is malformed")?;
    let event_name = required_env("EVENT_NAME")?;
    let repo = required_env("GITHUB_REPOSITORY")?;
    let source = capture_record(
        &event,
        &event_bytes,
        &event_name,
        &repo,
        &required_env("GITHUB_REF_NAME")?,
        &required_env("GITHUB_SHA")?,
        &required_env("GITHUB_WORKFLOW_SHA")?,
    )?;
    let root = PathBuf::from(runner_temp).join("dependency-remediation-capture");
    fs::create_dir_all(root.join("events"))?;
    write_private(&root.join("events/trigger-event.json"), &event_bytes)?;
    write_json(&root.join("source.json"), &source)?;
    append_output(
        &PathBuf::from(output_file),
        &format!(
            "source_sha={}\nbase_sha={}\nartifact_name=gh-steward-dependency-remediation-capture-{}-{}\n",
            source["source_sha"].as_str().unwrap_or_default(),
            source["base_sha"].as_str().unwrap_or_default(),
            required_env("GITHUB_RUN_ID")?,
            required_env("GITHUB_RUN_ATTEMPT")?
        ),
    )?;
    Ok(())
}

fn capture_record(
    event: &Value,
    event_bytes: &[u8],
    event_name: &str,
    repo: &str,
    ref_name: &str,
    sha: &str,
    workflow_sha: &str,
) -> Result<Value> {
    let event_sha = sha256(event_bytes);
    let (source_repository, source_ref, source_sha, base_ref, base_sha) =
        if event_name == "pull_request_target" {
            let source_repository = text_at(event, &["pull_request", "head", "repo", "full_name"])?;
            let source_ref = text_at(event, &["pull_request", "head", "ref"])?;
            let source_sha = text_at(event, &["pull_request", "head", "sha"])?;
            let base_ref = text_at(event, &["pull_request", "base", "ref"])?;
            let base_sha = text_at(event, &["pull_request", "base", "sha"])?;
            ensure!(
                source_repository == repo,
                "fork dependency updates are not eligible for trusted publication"
            );
            ensure!(
                text_at(event, &["pull_request", "base", "repo", "full_name"])? == repo,
                "pull request base repository differs from the workflow repository"
            );
            (
                source_repository.to_owned(),
                source_ref.to_owned(),
                source_sha.to_owned(),
                base_ref.to_owned(),
                base_sha.to_owned(),
            )
        } else if matches!(event_name, "schedule" | "workflow_dispatch") {
            (
                repo.to_owned(),
                ref_name.to_owned(),
                sha.to_owned(),
                ref_name.to_owned(),
                sha.to_owned(),
            )
        } else {
            bail!("unsupported dependency remediation event: {event_name}");
        };
    validate_sha40(&source_sha)?;
    validate_sha40(&base_sha)?;
    validate_sha40(workflow_sha)?;
    validate_sha256(&event_sha)?;
    for branch in [&source_ref, &base_ref] {
        let reference = format!("refs/heads/{branch}");
        let output = Command::new("git")
            .args(["check-ref-format", &reference])
            .output()
            .context("git could not validate the captured branch ref")?;
        ensure!(
            output.status.success(),
            "Git rejected an event-bound source or base ref"
        );
    }
    Ok(json!({
        "event_name":event_name,"source_repository":source_repository,
        "source_ref":source_ref,"source_sha":source_sha,"base_ref":base_ref,
        "base_sha":base_sha,"workflow_sha":workflow_sha,"trigger_event_sha256":event_sha
    }))
}

pub fn validate_candidate_inputs(capture_dir: &Path, proposal_dir: &Path) -> Result<()> {
    let capture_source_path = capture_dir.join("source.json");
    let proposal_source_path = proposal_dir.join("source.json");
    let capture_event_path = capture_dir.join("events/trigger-event.json");
    let proposal_event_path = proposal_dir.join("events/trigger-event.json");
    ensure!(
        fs::read(&capture_source_path)? == fs::read(&proposal_source_path)?,
        "proposal source identity differs from the exact capture"
    );
    ensure!(
        fs::read(capture_dir.join("source.patch"))? == fs::read(proposal_dir.join("source.patch"))?,
        "proposal source patch differs from the exact capture"
    );
    ensure!(
        fs::read(capture_dir.join("verification.json"))?
            == fs::read(proposal_dir.join("source-verification.json"))?,
        "proposal source verification differs from the exact capture"
    );
    ensure!(
        fs::read(&capture_event_path)? == fs::read(&proposal_event_path)?,
        "proposal event bytes differ from the exact capture"
    );
    let source = read_json(&capture_source_path)?;
    ensure!(
        source["event_name"] == required_env("GITHUB_EVENT_NAME")?
            && source["workflow_sha"] == required_env("GITHUB_WORKFLOW_SHA")?
            && source["source_repository"] == required_env("GITHUB_REPOSITORY")?,
        "captured source identity differs from the current trusted event"
    );
    let event_bytes = fs::read(&capture_event_path)?;
    ensure!(
        source["trigger_event_sha256"] == sha256(&event_bytes),
        "captured event digest does not match the exact event bytes"
    );
    validate_sha40(source["source_sha"].as_str().unwrap_or_default())?;
    validate_sha40(source["base_sha"].as_str().unwrap_or_default())?;
    let event: Value = serde_json::from_slice(&event_bytes)?;
    if source["event_name"] == "pull_request_target" {
        ensure!(
            source["source_ref"] == event["pull_request"]["head"]["ref"]
                && source["source_sha"] == event["pull_request"]["head"]["sha"]
                && source["source_repository"]
                    == event["pull_request"]["head"]["repo"]["full_name"]
                && source["base_ref"] == event["pull_request"]["base"]["ref"]
                && source["base_sha"] == event["pull_request"]["base"]["sha"]
                && event["pull_request"]["base"]["repo"]["full_name"]
                    == required_env("GITHUB_REPOSITORY")?,
            "captured PR source identity does not match its exact event"
        );
    } else {
        ensure!(
            source["source_ref"] == required_env("BASE_REF")?
                && source["source_sha"] == required_env("BASE_SHA")?
                && source["base_ref"] == required_env("BASE_REF")?
                && source["base_sha"] == required_env("BASE_SHA")?,
            "captured source does not match the exact scheduled or manual event"
        );
    }
    let verification: Value = read_json(&capture_dir.join("verification.json"))?;
    ensure!(
        verification["status"] == "passed"
            && verification["credential_free"] == true
            && verification["source_sha"] == source["source_sha"],
        "source verification is not a credential-free result for the exact source"
    );
    let result = read_result(&proposal_dir.join("publication/result.json"))?;
    ensure!(
        result["status"] == "patched",
        "Codex proposal is not a patched result"
    );
    append_output(
        &PathBuf::from(required_env("GITHUB_OUTPUT")?),
        &format!(
            "source_sha={}\n",
            source["source_sha"].as_str().unwrap_or_default()
        ),
    )?;
    Ok(())
}

fn read_json(path: &Path) -> Result<Value> {
    let metadata = fs::symlink_metadata(path)?;
    ensure!(
        metadata.is_file() && !metadata.file_type().is_symlink(),
        "source handoff JSON must be a regular file"
    );
    Ok(serde_json::from_slice(&fs::read(path)?)?)
}

fn write_json(path: &Path, value: &Value) -> Result<()> {
    let mut bytes = serde_json::to_vec_pretty(value)?;
    bytes.push(b'\n');
    write_private(path, &bytes)
}

fn write_private(path: &Path, bytes: &[u8]) -> Result<()> {
    let temp = path.with_extension(format!("tmp-{}", std::process::id()));
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(&temp)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    fs::rename(temp, path)?;
    Ok(())
}

fn append_output(path: &Path, text: &str) -> Result<()> {
    use std::fs::OpenOptions;
    let mut file = OpenOptions::new()
        .append(true)
        .open(path)
        .context("GitHub Actions output file is unavailable")?;
    file.write_all(text.as_bytes())?;
    Ok(())
}

fn text_at<'a>(value: &'a Value, path: &[&str]) -> Result<&'a str> {
    let mut current = value;
    for key in path {
        current = current
            .get(*key)
            .ok_or_else(|| anyhow::anyhow!("event is missing {}", path.join(".")))?;
    }
    current
        .as_str()
        .filter(|text| !text.is_empty())
        .ok_or_else(|| anyhow::anyhow!("event field {} is empty or invalid", path.join(".")))
}

fn required_env(name: &str) -> Result<String> {
    env::var(name).with_context(|| format!("{name} is required"))
}
fn sha256(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    hex::encode(Sha256::digest(bytes))
}
fn validate_sha40(value: &str) -> Result<()> {
    if value.len() != 40
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        bail!("source commit identity is malformed");
    }
    Ok(())
}
fn validate_sha256(value: &str) -> Result<()> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        bail!("event SHA-256 is malformed");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::capture_record;
    use serde_json::json;

    const BASE_SHA: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
    const SOURCE_SHA: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const WORKFLOW_SHA: &str = "cccccccccccccccccccccccccccccccccccccccc";

    #[test]
    fn pull_request_capture_is_bound_to_same_repo_head_and_base() {
        let event = json!({
            "pull_request": {
                "head": {
                    "repo": {"full_name":"owner/repo"},
                    "ref":"dependabot/cargo/example",
                    "sha":SOURCE_SHA
                },
                "base": {
                    "repo": {"full_name":"owner/repo"},
                    "ref":"main",
                    "sha":BASE_SHA
                }
            }
        });
        let bytes = serde_json::to_vec(&event).unwrap();
        let capture = capture_record(
            &event,
            &bytes,
            "pull_request_target",
            "owner/repo",
            "main",
            BASE_SHA,
            WORKFLOW_SHA,
        )
        .unwrap();

        assert_eq!(capture["source_repository"], "owner/repo");
        assert_eq!(capture["source_ref"], "dependabot/cargo/example");
        assert_eq!(capture["source_sha"], SOURCE_SHA);
        assert_eq!(capture["base_ref"], "main");
        assert_eq!(capture["base_sha"], BASE_SHA);
    }

    #[test]
    fn capture_rejects_forks_and_unsupported_events() {
        let event = json!({
            "pull_request": {
                "head": {"repo":{"full_name":"fork/repo"},"ref":"branch","sha":SOURCE_SHA},
                "base": {"repo":{"full_name":"owner/repo"},"ref":"main","sha":BASE_SHA}
            }
        });
        let bytes = serde_json::to_vec(&event).unwrap();
        assert!(
            capture_record(
                &event,
                &bytes,
                "pull_request_target",
                "owner/repo",
                "main",
                BASE_SHA,
                WORKFLOW_SHA,
            )
            .is_err()
        );
        assert!(
            capture_record(
                &event,
                &bytes,
                "issues",
                "owner/repo",
                "main",
                BASE_SHA,
                WORKFLOW_SHA,
            )
            .is_err()
        );
    }
}
