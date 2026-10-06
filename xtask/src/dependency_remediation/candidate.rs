use anyhow::{Context, Result, bail, ensure};
use serde_json::{Value, json};
use std::{env, fs, path::Path, process::Command};

use super::paths::{read_result, validate_result_paths};

pub fn apply(root: &Path, proposal: &Path, source_sha: &str, tree_file: &Path) -> Result<()> {
    validate_sha40(source_sha)?;
    ensure!(
        git_text(root, &["rev-parse", "HEAD"])? == source_sha,
        "candidate checkout is not the exact captured source commit"
    );
    ensure!(
        git_output(root, &["status", "--porcelain", "-z"])?.is_empty(),
        "candidate checkout must be clean before applying the supplemental patch"
    );
    let result_path = proposal.join("publication/result.json");
    let patch_path = proposal.join("publication/patch.diff");
    let result = read_result(&result_path)?;
    let mut expected = validate_result_paths(&result)?;
    expected.sort();
    require_regular(&patch_path)?;
    let patch = fs::read(&patch_path)?;
    ensure!(
        result["supplemental_patch"]
            .as_str()
            .unwrap_or_default()
            .as_bytes()
            == patch,
        "candidate patch bytes differ from the exact proposal result"
    );

    git(
        root,
        &[
            "-c",
            "core.hooksPath=/dev/null",
            "apply",
            "--index",
            "--check",
            "--binary",
            path_arg(&patch_path)?,
        ],
    )?;
    git(
        root,
        &[
            "-c",
            "core.hooksPath=/dev/null",
            "apply",
            "--index",
            "--binary",
            path_arg(&patch_path)?,
        ],
    )?;
    let changed = git_output(root, &["diff", "--cached", "--name-only", "-z", source_sha])?;
    let mut actual = changed
        .split(|byte| *byte == 0)
        .filter(|part| !part.is_empty())
        .map(|part| String::from_utf8(part.to_vec()))
        .collect::<std::result::Result<Vec<_>, _>>()
        .context("changed file names are not valid UTF-8")?;
    actual.sort();
    ensure!(
        actual == expected,
        "patch paths differ from the captured dependency remediation plan"
    );
    ensure!(
        git_output(root, &["diff", "--name-only", "-z"])?.is_empty(),
        "candidate worktree differs from the exact staged patch"
    );
    let tree = git_text(root, &["write-tree"])?;
    validate_sha40(&tree)?;
    write_private(tree_file, tree.as_bytes())?;
    Ok(())
}

pub fn verify(root: &Path, source_sha: &str, tree_file: &Path) -> Result<()> {
    validate_sha40(source_sha)?;
    require_regular(tree_file)?;
    let expected = fs::read_to_string(tree_file)?.trim().to_owned();
    validate_sha40(&expected)?;
    ensure!(
        git_text(root, &["rev-parse", "HEAD"])? == source_sha,
        "candidate tests changed the exact source commit"
    );
    ensure!(
        git_output(root, &["diff", "--name-only", "-z"])?.is_empty(),
        "candidate tests modified tracked source files"
    );
    ensure!(
        git_output(root, &["ls-files", "--others", "--exclude-standard", "-z"])?.is_empty(),
        "candidate tests created untracked source files"
    );
    let actual = git_text(root, &["write-tree"])?;
    ensure!(
        actual == expected,
        "candidate test execution changed the exact staged tree"
    );
    Ok(())
}

pub fn create_evidence(capture: &Path, proposal: &Path, destination: &Path) -> Result<()> {
    let runner_temp_path = required_env("RUNNER_TEMP")?;
    let runner_temp = Path::new(&runner_temp_path);
    ensure!(
        destination == runner_temp.join("dependency-remediation-candidate"),
        "candidate evidence destination must be the exact run-scoped directory"
    );
    ensure!(
        !destination.exists(),
        "candidate evidence destination already exists"
    );

    let source = read_json_regular(&capture.join("source.json"))?;
    let event = read_regular_bytes(&proposal.join("events/trigger-event.json"))?;
    let patch = read_regular_bytes(&proposal.join("publication/patch.diff"))?;
    let result_bytes = read_regular_bytes(&proposal.join("publication/result.json"))?;
    let result: Value = serde_json::from_slice(&result_bytes)?;
    let validated = read_result(&proposal.join("publication/result.json"))?;
    ensure!(
        validated == result,
        "proposal result changed during validation"
    );
    ensure!(
        result["supplemental_patch"]
            .as_str()
            .unwrap_or_default()
            .as_bytes()
            == patch,
        "candidate patch bytes differ from the exact proposal result"
    );

    let workflow_sha = required_env("GITHUB_WORKFLOW_SHA")?;
    let workflow_file = "dependency-remediation.yml";
    let event_name = required_env("GITHUB_EVENT_NAME")?;
    let repository = required_env("GITHUB_REPOSITORY")?;
    let server_url = required_env("GITHUB_SERVER_URL")?;
    let run_id = positive_env("GITHUB_RUN_ID")?;
    let run_attempt = positive_env("GITHUB_RUN_ATTEMPT")?;
    let capture_artifact = artifact_from_env("CAPTURE")?;
    let proposal_artifact = artifact_from_env("PROPOSAL")?;
    let candidate_tree_sha = required_env("CANDIDATE_TREE")?;
    validate_sha40(&candidate_tree_sha)?;
    ensure!(
        source["event_name"] == event_name
            && source["workflow_sha"] == workflow_sha
            && source["source_repository"] == repository,
        "captured source identity differs from the trusted candidate event"
    );
    validate_sha40(source["source_sha"].as_str().unwrap_or_default())?;
    validate_sha40(source["base_sha"].as_str().unwrap_or_default())?;
    let event_sha = sha256(&event);
    ensure!(
        source["trigger_event_sha256"] == event_sha,
        "candidate event bytes differ from the source capture"
    );

    let record = candidate_record(
        &source,
        &repository,
        &server_url,
        workflow_file,
        &workflow_sha,
        &event_name,
        run_id,
        run_attempt,
        &candidate_tree_sha,
        &patch,
        &result_bytes,
        &capture_artifact,
        &proposal_artifact,
    )?;

    fs::create_dir_all(destination.join("publication"))?;
    fs::create_dir_all(destination.join("events"))?;
    write_private(&destination.join("publication/patch.diff"), &patch)?;
    write_private(&destination.join("publication/result.json"), &result_bytes)?;
    write_private(&destination.join("events/trigger-event.json"), &event)?;
    let mut record_bytes = serde_json::to_vec_pretty(&record)?;
    record_bytes.push(b'\n');
    write_private(
        &destination.join("publication/candidate.json"),
        &record_bytes,
    )?;
    append_output(
        Path::new(&required_env("GITHUB_OUTPUT")?),
        &format!("artifact_name=gh-steward-candidate-{run_id}-{run_attempt}\n"),
    )?;
    println!("exact dependency-remediation candidate evidence created");
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn candidate_record(
    source: &Value,
    repository: &str,
    server_url: &str,
    workflow_file: &str,
    workflow_sha: &str,
    event_name: &str,
    run_id: u64,
    run_attempt: u64,
    candidate_tree_sha: &str,
    patch: &[u8],
    result: &[u8],
    capture: &Value,
    proposal: &Value,
) -> Result<Value> {
    Ok(json!({
        "schema_version":1,
        "repository":repository,
        "server_url":server_url,
        "workflow_file":workflow_file,
        "origin_run_id":run_id,
        "origin_run_attempt":run_attempt,
        "workflow_sha":workflow_sha,
        "event_name":event_name,
        "trigger_event_sha256":source["trigger_event_sha256"],
        "source_repository":source["source_repository"],
        "source_ref":source["source_ref"],
        "source_sha":source["source_sha"],
        "base_ref":source["base_ref"],
        "base_sha":source["base_sha"],
        "candidate_tree_sha":candidate_tree_sha,
        "patch_sha256":sha256(patch),
        "result_sha256":sha256(result),
        "upstream_artifacts":{
            "capture":capture,
            "proposal":proposal
        },
        "verification_job_name":"Verify publication candidate"
    }))
}

fn artifact_from_env(prefix: &str) -> Result<Value> {
    let id = positive_env(&format!("{prefix}_ID"))?;
    let name = required_env(&format!("{prefix}_NAME"))?;
    let digest = required_env(&format!("{prefix}_DIGEST"))?;
    ensure!(
        digest.strip_prefix("sha256:").is_some_and(is_sha256),
        "candidate upstream artifact digest is malformed"
    );
    ensure!(
        !name.is_empty(),
        "candidate upstream artifact name is empty"
    );
    Ok(json!({"id":id,"name":name,"digest":digest}))
}

fn read_json_regular(path: &Path) -> Result<Value> {
    let bytes = read_regular_bytes(path)?;
    Ok(serde_json::from_slice(&bytes)?)
}

fn read_regular_bytes(path: &Path) -> Result<Vec<u8>> {
    let metadata = fs::symlink_metadata(path)?;
    ensure!(
        metadata.is_file() && !metadata.file_type().is_symlink(),
        "candidate evidence input must be a regular file"
    );
    Ok(fs::read(path)?)
}

fn positive_env(name: &str) -> Result<u64> {
    let value = required_env(name)?;
    let parsed = value.parse::<u64>()?;
    ensure!(
        parsed > 0 && parsed.to_string() == value,
        "{name} must be a positive integer"
    );
    Ok(parsed)
}

fn required_env(name: &str) -> Result<String> {
    env::var(name).with_context(|| format!("{name} is required"))
}

fn append_output(path: &Path, value: &str) -> Result<()> {
    use std::io::Write;
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    file.write_all(value.as_bytes())?;
    Ok(())
}

fn sha256(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    hex::encode(Sha256::digest(bytes))
}

fn is_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}

fn git(root: &Path, args: &[&str]) -> Result<()> {
    let output = git_command(root, args)?;
    ensure!(
        output.status.success(),
        "git rejected the exact candidate patch"
    );
    Ok(())
}

fn git_output(root: &Path, args: &[&str]) -> Result<Vec<u8>> {
    let output = git_command(root, args)?;
    ensure!(
        output.status.success(),
        "git could not inspect the exact candidate"
    );
    Ok(output.stdout)
}

fn git_command(root: &Path, args: &[&str]) -> Result<std::process::Output> {
    Command::new("git")
        .args(args)
        .current_dir(root)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_SYSTEM", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .context("git command could not be started")
}

fn git_text(root: &Path, args: &[&str]) -> Result<String> {
    Ok(String::from_utf8(git_output(root, args)?)?
        .trim()
        .to_owned())
}

fn require_regular(path: &Path) -> Result<()> {
    let metadata = fs::symlink_metadata(path)?;
    ensure!(
        metadata.is_file() && !metadata.file_type().is_symlink(),
        "candidate input must be a regular file"
    );
    Ok(())
}

fn write_private(path: &Path, bytes: &[u8]) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let temp = path.with_extension(format!("tmp-{}", std::process::id()));
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    use std::io::Write;
    let mut file = options.open(&temp)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    fs::rename(temp, path)?;
    Ok(())
}

fn path_arg(path: &Path) -> Result<&str> {
    path.to_str()
        .ok_or_else(|| anyhow::anyhow!("candidate path must be valid UTF-8"))
}

fn validate_sha40(value: &str) -> Result<()> {
    if value.len() != 40
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        bail!("candidate commit identity is malformed");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::candidate_record;
    use serde_json::json;

    #[test]
    fn evidence_record_binds_exact_source_tree_payload_and_artifact_identities() {
        let source = json!({
            "event_name":"pull_request_target",
            "trigger_event_sha256":"event-digest",
            "source_repository":"owner/repo",
            "source_ref":"dependabot/update",
            "source_sha":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "base_ref":"main",
            "base_sha":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
        });
        let capture = json!({"id":7,"name":"capture","digest":"sha256:capture"});
        let proposal = json!({"id":9,"name":"proposal","digest":"sha256:proposal"});
        let patch = b"exact patch bytes";
        let result = b"exact result bytes";

        let record = candidate_record(
            &source,
            "owner/repo",
            "https://github.com",
            "dependency-remediation.yml",
            "cccccccccccccccccccccccccccccccccccccccc",
            "pull_request_target",
            11,
            2,
            "dddddddddddddddddddddddddddddddddddddddd",
            patch,
            result,
            &capture,
            &proposal,
        )
        .unwrap();

        assert_eq!(record["source_sha"], source["source_sha"]);
        assert_eq!(record["base_sha"], source["base_sha"]);
        assert_eq!(
            record["candidate_tree_sha"],
            "dddddddddddddddddddddddddddddddddddddddd"
        );
        assert_eq!(record["patch_sha256"], super::sha256(patch));
        assert_eq!(record["result_sha256"], super::sha256(result));
        assert_eq!(record["upstream_artifacts"]["capture"]["id"], 7);
        assert_eq!(record["upstream_artifacts"]["proposal"]["id"], 9);
    }
}
