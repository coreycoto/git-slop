use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::{env, fs, io::Write, path::Path};

use super::artifacts::{parse_positive_id, valid_digest};
use super::paths::read_result;

pub fn collect(
    capture: &Path,
    result_path: &Path,
    destination: &Path,
    workflow_sha: &str,
    capture_id: &str,
    capture_name: &str,
    capture_digest: &str,
) -> Result<()> {
    let runner_temp_value = required_env("RUNNER_TEMP")?;
    let runner_temp = Path::new(&runner_temp_value);
    ensure!(
        capture == runner_temp.join("dependency-remediation-source")
            && result_path == runner_temp.join("codex/dependency-remediation.json")
            && destination == runner_temp.join("dependency-remediation-proposal"),
        "proposal inputs and destination must be exact run-scoped paths"
    );
    let result: Value = serde_json::from_slice(&read_regular(result_path)?)
        .context("Codex proposal is malformed JSON")?;
    if classify(&result)? == ProposalKind::Noop {
        append_output("ready=false\nnoop_reason=no-change\n")?;
        append_summary("Codex found no safe additional remediation; publication was skipped.\n")?;
        return Ok(());
    }
    let validated = read_result(result_path)?;
    ensure!(
        validated == result,
        "Codex proposal changed during validation"
    );
    ensure!(
        workflow_sha.len() == 40
            && workflow_sha
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase()),
        "trusted workflow identity is malformed"
    );
    parse_positive_id(capture_id)?;
    ensure!(
        !capture_name.is_empty(),
        "capture artifact name is required"
    );
    ensure!(
        valid_digest(capture_digest),
        "capture artifact digest is malformed"
    );
    ensure!(
        !destination.exists(),
        "proposal handoff destination already exists"
    );

    fs::create_dir_all(destination.join("publication"))?;
    fs::create_dir_all(destination.join("events"))?;
    copy_exact(
        &capture.join("source.json"),
        &destination.join("source.json"),
    )?;
    copy_exact(
        &capture.join("source.patch"),
        &destination.join("source.patch"),
    )?;
    copy_exact(
        &capture.join("verification.json"),
        &destination.join("source-verification.json"),
    )?;
    copy_exact(
        &capture.join("events/trigger-event.json"),
        &destination.join("events/trigger-event.json"),
    )?;
    let mut result_bytes = serde_json::to_vec_pretty(&result)?;
    result_bytes.push(b'\n');
    write_private(&destination.join("publication/result.json"), &result_bytes)?;
    write_private(
        &destination.join("publication/patch.diff"),
        result["supplemental_patch"]
            .as_str()
            .unwrap_or_default()
            .as_bytes(),
    )?;
    write_json(
        &destination.join("proposal.json"),
        &json!({
            "schema_version":1,
            "workflow_sha":workflow_sha,
            "capture":{"id":capture_id,"name":capture_name,"digest":capture_digest}
        }),
    )?;
    append_output(&format!(
        "ready=true\nartifact_name=gh-steward-dependency-remediation-proposal-{}-{}\n",
        required_env("GITHUB_RUN_ID")?,
        required_env("GITHUB_RUN_ATTEMPT")?
    ))?;
    Ok(())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ProposalKind {
    Patched,
    Noop,
}

fn classify(result: &Value) -> Result<ProposalKind> {
    if result.is_object()
        && result["status"] == "patched"
        && result["supplemental_patch"]
            .as_str()
            .is_some_and(|patch| !patch.is_empty())
    {
        return Ok(ProposalKind::Patched);
    }
    ensure!(
        result["status"] == "noop",
        "Codex did not produce a bounded proposal"
    );
    Ok(ProposalKind::Noop)
}

fn copy_exact(source: &Path, destination: &Path) -> Result<()> {
    let bytes = read_regular(source)?;
    write_private(destination, &bytes)
}

fn read_regular(path: &Path) -> Result<Vec<u8>> {
    let metadata = fs::symlink_metadata(path)?;
    ensure!(
        metadata.is_file() && !metadata.file_type().is_symlink(),
        "proposal handoff input must be a regular file"
    );
    Ok(fs::read(path)?)
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

fn append_output(value: &str) -> Result<()> {
    let output = required_env("GITHUB_OUTPUT")?;
    let mut file = fs::OpenOptions::new().append(true).open(output)?;
    file.write_all(value.as_bytes())?;
    Ok(())
}

fn append_summary(value: &str) -> Result<()> {
    let summary = required_env("GITHUB_STEP_SUMMARY")?;
    let mut file = fs::OpenOptions::new().append(true).open(summary)?;
    file.write_all(value.as_bytes())?;
    Ok(())
}

fn required_env(name: &str) -> Result<String> {
    env::var(name).with_context(|| format!("{name} is required"))
}

#[cfg(test)]
mod tests {
    use super::{ProposalKind, classify, copy_exact, read_regular};
    use serde_json::json;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn exact_handoff_copy_preserves_patch_and_event_bytes_and_rejects_symlinks() {
        let temp = TempDir::new().unwrap();
        let source = temp.path().join("source");
        let destination = temp.path().join("destination");
        fs::create_dir_all(&source).unwrap();
        let patch = b"diff --git a/src/lib.rs b/src/lib.rs\n\0\xff";
        fs::write(source.join("source.patch"), patch).unwrap();
        copy_exact(&source.join("source.patch"), &destination).unwrap();
        assert_eq!(fs::read(&destination).unwrap(), patch);

        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(source.join("source.patch"), source.join("link")).unwrap();
            assert!(read_regular(&source.join("link")).is_err());
        }
    }

    #[test]
    fn proposal_collection_distinguishes_only_valid_patched_and_noop_states() {
        assert_eq!(
            classify(&json!({"status":"patched","supplemental_patch":"diff"})).unwrap(),
            ProposalKind::Patched
        );
        assert_eq!(
            classify(&json!({"status":"noop"})).unwrap(),
            ProposalKind::Noop
        );
        for invalid in [
            json!(null),
            json!({"status":"patched","supplemental_patch":""}),
            json!({"status":"failed","supplemental_patch":"diff"}),
        ] {
            assert!(classify(&invalid).is_err());
        }
    }
}
