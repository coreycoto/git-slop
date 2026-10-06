use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::{env, fs, io::Write, path::Path};

pub fn prepare(capture: &Path, package: &Path, context: &Path) -> Result<()> {
    let runner_temp_value = required_env("RUNNER_TEMP")?;
    let runner_temp = Path::new(&runner_temp_value);
    ensure!(
        capture == runner_temp.join("dependency-remediation-noop-capture")
            && package == runner_temp.join("dependency-remediation-package")
            && context == runner_temp.join("dependency-remediation-noop-context.json"),
        "no-op inputs and destinations must be exact run-scoped paths"
    );
    let event_path_value = required_env("GITHUB_EVENT_PATH")?;
    let event_path = Path::new(&event_path_value);
    prepare_inputs(
        capture,
        package,
        context,
        &read_regular(event_path)?,
        &NoopIdentity {
            decision: required_env("NOOP_DECISION")?,
            repository: required_env("GITHUB_REPOSITORY")?,
            server_url: required_env("GITHUB_SERVER_URL")?,
            event_name: required_env("GITHUB_EVENT_NAME")?,
            workflow_sha: required_env("GITHUB_WORKFLOW_SHA")?,
            run_id: required_env("GITHUB_RUN_ID")?,
            attempt: required_env("GITHUB_RUN_ATTEMPT")?,
        },
    )
}

struct NoopIdentity {
    decision: String,
    repository: String,
    server_url: String,
    event_name: String,
    workflow_sha: String,
    run_id: String,
    attempt: String,
}

fn prepare_inputs(
    capture: &Path,
    package: &Path,
    context: &Path,
    current_event: &[u8],
    identity: &NoopIdentity,
) -> Result<()> {
    ensure!(
        matches!(
            identity.decision.as_str(),
            "no-change" | "prerequisite-unavailable"
        ),
        "no-op decision is outside the declared workflow contract"
    );
    let run_id = parse_positive(&identity.run_id, "workflow run ID")?;
    let attempt = parse_positive(&identity.attempt, "workflow run attempt")?;
    validate_sha40(&identity.workflow_sha)?;
    let source_path = capture.join("source.json");
    let event_path = capture.join("events/trigger-event.json");
    let source: Value = serde_json::from_slice(&read_regular(&source_path)?)
        .context("source capture identity is malformed")?;
    let event = read_regular(&event_path)?;
    ensure!(
        event == current_event,
        "captured event bytes differ from the current trusted event"
    );
    let event_sha = sha256(&event);
    ensure!(
        source["event_name"] == identity.event_name
            && source["workflow_sha"] == identity.workflow_sha
            && source["source_repository"] == identity.repository
            && source["trigger_event_sha256"] == event_sha,
        "captured source identity differs from the current trusted event"
    );
    let source_sha = source["source_sha"].as_str().unwrap_or_default();
    let base_sha = source["base_sha"].as_str().unwrap_or_default();
    validate_sha40(source_sha)?;
    validate_sha40(base_sha)?;

    fs::create_dir_all(package.join("events"))?;
    fs::create_dir_all(package.join("decisions"))?;
    write_private(&package.join("events/trigger-event.json"), &event)?;
    write_json(
        context,
        &json!({
            "repository":identity.repository,
            "workflow_file":"dependency-remediation.yml",
            "recovery_key":"workflow-history-v2",
            "run_name":"Dependency Remediation",
            "run_id":run_id,
            "attempt":attempt,
            "attempt_target":{
                "trigger_event_sha256":event_sha,
                "source_sha":source_sha,
                "base_sha":base_sha
            },
            "dispatch_steps":[],
            "trusted_source_sha":identity.workflow_sha
        }),
    )?;
    write_json(
        &package.join("decisions/workflow-noop.json"),
        &json!({
            "schema_version":1,
            "decision":identity.decision,
            "repository":identity.repository,
            "server_url":identity.server_url,
            "workflow_file":"dependency-remediation.yml",
            "run_id":run_id,
            "attempt":attempt,
            "recovery_key":"workflow-history-v2",
            "attempt_target":{
                "trigger_event_sha256":event_sha,
                "source_sha":source_sha,
                "base_sha":base_sha
            },
            "workflow_sha":identity.workflow_sha,
            "event_name":identity.event_name,
            "event_sha256":event_sha
        }),
    )
}

fn parse_positive(value: &str, label: &str) -> Result<u64> {
    let number = value
        .parse::<u64>()
        .with_context(|| format!("{label} must be a positive integer"))?;
    ensure!(
        number > 0 && number.to_string() == value,
        "{label} is not canonical"
    );
    Ok(number)
}

fn validate_sha40(value: &str) -> Result<()> {
    ensure!(
        value.len() == 40
            && value
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase()),
        "source identity is malformed"
    );
    Ok(())
}

fn sha256(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    hex::encode(Sha256::digest(bytes))
}

fn read_regular(path: &Path) -> Result<Vec<u8>> {
    let metadata = fs::symlink_metadata(path)?;
    ensure!(
        metadata.is_file() && !metadata.file_type().is_symlink(),
        "no-op evidence must be a regular file"
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

fn required_env(name: &str) -> Result<String> {
    env::var(name).with_context(|| format!("{name} is required"))
}

#[cfg(test)]
mod tests {
    use super::{NoopIdentity, prepare_inputs};
    use serde_json::{Value, json};
    use sha2::{Digest, Sha256};
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn noop_inputs_bind_event_bytes_and_exact_source_identity() {
        let temp = TempDir::new().unwrap();
        let capture = temp.path().join("capture");
        let package = temp.path().join("package");
        let context = temp.path().join("context.json");
        fs::create_dir_all(capture.join("events")).unwrap();
        let event = br#"{"event":"schedule"}"#;
        let event_sha = hex::encode(Sha256::digest(event));
        let source = json!({
            "event_name":"schedule",
            "workflow_sha":"cccccccccccccccccccccccccccccccccccccccc",
            "source_repository":"owner/repo",
            "source_sha":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "base_sha":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
            "trigger_event_sha256":event_sha
        });
        fs::write(
            capture.join("source.json"),
            serde_json::to_vec(&source).unwrap(),
        )
        .unwrap();
        fs::write(capture.join("events/trigger-event.json"), event).unwrap();
        let identity = NoopIdentity {
            decision: "no-change".into(),
            repository: "owner/repo".into(),
            server_url: "https://github.com".into(),
            event_name: "schedule".into(),
            workflow_sha: "cccccccccccccccccccccccccccccccccccccccc".into(),
            run_id: "123".into(),
            attempt: "2".into(),
        };

        prepare_inputs(&capture, &package, &context, event, &identity).unwrap();
        assert_eq!(
            fs::read(package.join("events/trigger-event.json")).unwrap(),
            event
        );
        let context_value: Value = serde_json::from_slice(&fs::read(&context).unwrap()).unwrap();
        assert_eq!(context_value["run_id"], 123);
        assert_eq!(context_value["attempt"], 2);
        assert_eq!(
            context_value["attempt_target"]["trigger_event_sha256"],
            event_sha
        );
        let decision: Value = serde_json::from_slice(
            &fs::read(package.join("decisions/workflow-noop.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(decision["decision"], "no-change");
        assert_eq!(decision["event_sha256"], event_sha);

        let changed_event = br#"{"event":"different"}"#;
        assert!(prepare_inputs(&capture, &package, &context, changed_event, &identity).is_err());
    }
}
