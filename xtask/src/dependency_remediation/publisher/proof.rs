use anyhow::{Result, bail, ensure};
use serde_json::{Value, json};
use std::{fs, path::Path};

use super::super::paths::{read_result, validate_result_paths};
use super::Package;
use super::common::*;
use super::git::*;

pub(super) fn read_publication(root: &Path, package: &Package) -> Result<(Value, Value)> {
    require_regular(
        &package.intent,
        "immutable publication intent is missing or unsafe",
    )?;
    let context = read_json(&package.context)?;
    let intent_sha = canonical_sha(root, &package.intent)?;
    ensure!(
        intent_sha == string_at(&context, &["publication", "intent_sha256"])?,
        "immutable publication intent digest mismatch"
    );
    let candidate = validate_candidate(package, false)?;
    let intent = read_json(&package.intent)?;
    let expected = [
        "schema_version",
        "repository",
        "repository_node_id",
        "workflow_file",
        "recovery_key",
        "workflow_sha",
        "candidate_sha256",
        "run_name",
        "origin_run_id",
        "origin_run_attempt",
        "event_name",
        "trigger_event_sha256",
        "source_repository",
        "source_ref",
        "source_sha",
        "target_pr",
        "mode",
        "base_ref",
        "base_sha",
        "head_ref",
        "expected_old_sha",
        "new_sha",
        "publisher_login",
        "title",
        "body",
        "draft",
        "nonce",
        "patch_sha256",
        "result_sha256",
        "files",
        "created_at",
    ];
    ensure!(
        exact_keys(&intent, &expected),
        "publication intent fields differ from the trusted contract"
    );
    ensure!(
        intent["schema_version"] == 2
            && intent["repository"] == required_env("GITHUB_REPOSITORY")?
            && intent["workflow_file"] == required_env("WORKFLOW_FILE")?
            && intent["recovery_key"] == required_env("RECOVERY_KEY")?
            && intent["run_name"] == required_env("WORKFLOW_RUN_NAME")?,
        "publication intent does not match the exact workflow identity"
    );
    ensure!(
        intent["workflow_sha"] == candidate["workflow_sha"]
            && intent["source_sha"] == candidate["source_sha"]
            && intent["base_ref"] == candidate["base_ref"]
            && intent["base_sha"] == candidate["base_sha"]
            && intent["trigger_event_sha256"] == candidate["trigger_event_sha256"]
            && intent["patch_sha256"] == candidate["patch_sha256"]
            && intent["result_sha256"] == candidate["result_sha256"],
        "publication intent differs from the exact verified candidate"
    );
    ensure!(
        intent["candidate_sha256"] == sha256(&fs::read(&package.candidate)?)
            && intent["origin_run_id"] == candidate["origin_run_id"]
            && intent["origin_run_attempt"] == candidate["origin_run_attempt"],
        "publication candidate identity changed"
    );
    ensure!(
        matches!(
            intent["mode"].as_str(),
            Some("update-existing-pr" | "create-pr")
        ),
        "publication mode is unsupported"
    );
    ensure!(
        intent["stage"].is_null(),
        "publication intent must remain immutable and stage-free"
    );
    Ok((context, intent))
}

pub(super) fn validate_files(package: &Package) -> Result<()> {
    let result = read_result(&package.result)?;
    require_regular(
        &package.patch,
        "captured dependency patch is missing or unsafe",
    )?;
    ensure!(
        result["supplemental_patch"]
            .as_str()
            .unwrap_or_default()
            .as_bytes()
            == fs::read(&package.patch)?,
        "captured patch bytes differ from the exact Codex result"
    );
    validate_result_paths(&result)?;
    Ok(())
}

pub(super) fn validate_candidate(package: &Package, live_event: bool) -> Result<Value> {
    require_regular(
        &package.candidate,
        "verified publication candidate is missing or unsafe",
    )?;
    validate_files(package)?;
    let candidate = read_json(&package.candidate)?;
    let expected = [
        "base_ref",
        "base_sha",
        "candidate_tree_sha",
        "event_name",
        "origin_run_attempt",
        "origin_run_id",
        "patch_sha256",
        "repository",
        "result_sha256",
        "schema_version",
        "server_url",
        "source_ref",
        "source_repository",
        "source_sha",
        "trigger_event_sha256",
        "upstream_artifacts",
        "verification_job_name",
        "workflow_file",
        "workflow_sha",
    ];
    ensure!(
        exact_keys(&candidate, &expected),
        "candidate fields differ from the trusted publication contract"
    );
    let repo = required_env("GITHUB_REPOSITORY")?;
    let server = required_env("GITHUB_SERVER_URL")?;
    let workflow = required_env("WORKFLOW_FILE")?;
    let event_name = required_env("GITHUB_EVENT_NAME")?;
    let actual_event = sha256(&fs::read(&package.event)?);
    let actual_patch = sha256(&fs::read(&package.patch)?);
    let actual_result = sha256(&fs::read(&package.result)?);
    let workflow_sha = required_env("GITHUB_WORKFLOW_SHA")?;
    ensure!(
        candidate["schema_version"] == 1
            && candidate["repository"] == repo
            && candidate["server_url"] == server
            && candidate["workflow_file"] == workflow
            && candidate["event_name"] == event_name,
        "candidate does not match the exact trusted workflow identity"
    );
    ensure!(
        candidate["trigger_event_sha256"] == actual_event
            && candidate["patch_sha256"] == actual_patch
            && candidate["result_sha256"] == actual_result,
        "candidate does not match exact trigger, patch and result bytes"
    );
    ensure!(
        candidate["source_repository"]
            .as_str()
            .is_some_and(|v| !v.is_empty())
            && candidate["source_ref"]
                .as_str()
                .is_some_and(|v| !v.is_empty()),
        "candidate source identity is incomplete"
    );
    for field in [
        "source_sha",
        "base_sha",
        "candidate_tree_sha",
        "workflow_sha",
    ] {
        validate_sha40(candidate[field].as_str().unwrap_or_default())?;
    }
    ensure!(
        candidate["verification_job_name"] == "Verify publication candidate",
        "candidate verification job identity is invalid"
    );
    let origin_run = positive_u64(&candidate["origin_run_id"])?;
    let origin_attempt = positive_u64(&candidate["origin_run_attempt"])?;
    let upstream = &candidate["upstream_artifacts"];
    ensure!(
        exact_keys(upstream, &["capture", "proposal"]),
        "candidate upstream artifacts differ from the trusted contract"
    );
    for name in ["capture", "proposal"] {
        let artifact = &upstream[name];
        ensure!(
            exact_keys(artifact, &["digest", "id", "name"])
                && positive_u64(&artifact["id"]).is_ok()
                && artifact["digest"].as_str().is_some_and(is_sha256_artifact)
                && artifact["name"].as_str().is_some_and(|v| !v.is_empty()),
            "candidate upstream artifact identity is invalid"
        );
        ensure!(
            artifact["name"]
                == format!(
                    "gh-steward-dependency-remediation-{name}-{origin_run}-{origin_attempt}"
                ),
            "candidate upstream artifact name is not bound to the exact attempt"
        );
    }
    if live_event {
        ensure!(
            candidate["origin_run_id"] == required_env("GITHUB_RUN_ID")?.parse::<u64>()?
                && candidate["origin_run_attempt"]
                    == required_env("GITHUB_RUN_ATTEMPT")?.parse::<u64>()?
                && candidate["workflow_sha"] == workflow_sha,
            "candidate is not from the current exact workflow attempt"
        );
        let live_event = fs::read(required_env("GITHUB_EVENT_PATH")?)?;
        ensure!(
            sha256(&live_event) == actual_event,
            "captured trigger bytes differ from the live event"
        );
        match event_name.as_str() {
            "pull_request_target" => {
                let event = read_json(&package.event)?;
                ensure!(
                    candidate["source_repository"]
                        == event["pull_request"]["head"]["repo"]["full_name"]
                        && candidate["source_ref"] == event["pull_request"]["head"]["ref"]
                        && candidate["source_sha"] == event["pull_request"]["head"]["sha"]
                        && candidate["base_ref"] == event["pull_request"]["base"]["ref"]
                        && candidate["base_sha"] == event["pull_request"]["base"]["sha"],
                    "candidate source identity differs from the exact pull request event"
                );
                ensure!(
                    event["pull_request"]["base"]["repo"]["full_name"] == repo,
                    "candidate base repository differs from the workflow repository"
                );
            }
            "schedule" | "workflow_dispatch" => {
                ensure!(
                    candidate["source_sha"] == required_env("GITHUB_SHA")?
                        && candidate["base_sha"] == required_env("GITHUB_SHA")?
                        && candidate["source_ref"] == required_env("GITHUB_REF_NAME")?
                        && candidate["base_ref"] == required_env("GITHUB_REF_NAME")?,
                    "candidate source differs from the exact scheduled or manual event"
                );
            }
            _ => bail!("candidate event type is unsupported"),
        }
    }
    validate_git_refs(&[
        &format!("refs/heads/{}", value_string(&candidate, "source_ref")?),
        &format!("refs/heads/{}", value_string(&candidate, "base_ref")?),
    ])?;
    Ok(candidate)
}

pub(super) fn validate_context(package: &Package) -> Result<()> {
    let context = read_json(&package.context)?;
    let repo = required_env("GITHUB_REPOSITORY")?;
    let workflow = required_env("WORKFLOW_FILE")?;
    let key = required_env("RECOVERY_KEY")?;
    let run_name = required_env("WORKFLOW_RUN_NAME")?;
    ensure!(
        context["schema_version"] == 1
            && context["repository"] == repo
            && context["workflow_file"] == workflow
            && context["recovery_key"] == key
            && context["run_name"] == run_name,
        "run context does not match the exact repository and workflow"
    );
    positive_u64(&context["workflow_run_id"])?;
    positive_u64(&context["workflow_run_attempt"])?;
    ensure!(
        matches!(
            context["phase"].as_str(),
            Some("started" | "prepared" | "completed" | "noop" | "recovery_needed")
        ) && context["plans"].is_array(),
        "run context has an unsupported phase or plan list"
    );
    Ok(())
}

pub(super) fn write_context(
    package: &Package,
    context: &mut Value,
    stage: &str,
    push_ack: Option<Value>,
    pr_ack: Option<Value>,
    verify_ack: Option<Value>,
    steps: Value,
) -> Result<()> {
    context["phase"] = json!(if stage == "completed" {
        "completed"
    } else {
        "prepared"
    });
    context["dispatch_steps"] = steps;
    context["publication"]["stage"] = json!(stage);
    context["publication"]["push_ack"] = push_ack.unwrap_or(Value::Null);
    context["publication"]["pr_ack"] = pr_ack.unwrap_or(Value::Null);
    context["publication"]["verify_ack"] = verify_ack.unwrap_or(Value::Null);
    write_json_new_or_replace(&package.context, context)
}

pub(super) fn safe_repo_identity() -> Result<()> {
    let repo = required_env("GITHUB_REPOSITORY")?;
    let parts: Vec<_> = repo.split('/').collect();
    ensure!(
        parts.len() == 2
            && parts.iter().all(|part| !part.is_empty()
                && part
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-'))),
        "GITHUB_REPOSITORY is invalid"
    );
    let server = required_env("GITHUB_SERVER_URL")?;
    ensure!(
        server.starts_with("https://")
            && server[8..]
                .trim_end_matches('/')
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | ':'))
            && !server[8..].trim_end_matches('/').is_empty(),
        "GITHUB_SERVER_URL must be an HTTPS origin"
    );
    positive_integer(&required_env("GITHUB_RUN_ID")?)?;
    positive_integer(&required_env("GITHUB_RUN_ATTEMPT")?)?;
    ensure!(
        required_env("WORKFLOW_FILE")? == "dependency-remediation.yml",
        "WORKFLOW_FILE must identify the trusted dependency-remediation workflow"
    );
    for name in ["WORKFLOW_RUN_NAME", "RECOVERY_KEY"] {
        let value = required_env(name)?;
        ensure!(
            !value.contains(['\n', '\r']),
            "workflow run identity is invalid"
        );
    }
    let started = required_env("WORKFLOW_RUN_STARTED_AT")?;
    ensure!(
        started.len() == 20
            && started.ends_with('Z')
            && started.chars().enumerate().all(|(i, c)| match i {
                4 | 7 => c == '-',
                10 => c == 'T',
                13 | 16 => c == ':',
                19 => c == 'Z',
                _ => c.is_ascii_digit(),
            }),
        "WORKFLOW_RUN_STARTED_AT is invalid"
    );
    Ok(())
}
