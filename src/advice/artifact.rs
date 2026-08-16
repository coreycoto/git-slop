use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use chrono::Utc;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::policy::CompiledPolicySet;
use crate::text::visible_controls;

use super::{ADVICE_SCHEMA_VERSION, ProviderResult, ValidatedResponse};

pub struct AdviceRun {
    pub artifact: Value,
    pub markdown: String,
}

pub struct AdviceTimings {
    pub context_elapsed_ms: u128,
    pub provider_elapsed_ms: u128,
    pub validation_elapsed_ms: u128,
    pub time_to_validated_artifact_ms: u128,
}

fn sha256(bytes: impl AsRef<[u8]>) -> String {
    hex::encode(Sha256::digest(bytes.as_ref()))
}

impl AdviceRun {
    pub fn new(
        input: &Value,
        policies: &CompiledPolicySet,
        provider: &ProviderResult,
        validated: &ValidatedResponse,
        timings: AdviceTimings,
    ) -> Result<Self> {
        let generated_at = Utc::now().to_rfc3339();
        let response_digest = sha256(serde_json::to_vec(&provider.response)?);
        let artifact = json!({
            "schema_version": ADVICE_SCHEMA_VERSION,
            "command": "advise",
            "generated_at": generated_at,
            "report": input.get("report").cloned().unwrap_or(Value::Null),
            "selector": input.get("selector").cloned().unwrap_or(Value::Null),
            "candidate_ids": input.pointer("/reference_index/candidates").cloned().unwrap_or_else(|| json!([])),
            "context": {
                "builder_version": input.get("context_builder_version").cloned().unwrap_or(Value::Null),
                "digest": input.get("context_digest").cloned().unwrap_or(Value::Null),
                "limits": input.get("limits").cloned().unwrap_or(Value::Null),
                "missing_evidence": input.get("missing_evidence").cloned().unwrap_or_else(|| json!([])),
                "reference_index": input.get("reference_index").cloned().unwrap_or(Value::Null),
            },
            "policies": {
                "resolution_digest": &policies.resolution_digest,
                "packs": &policies.packs,
                "conflicts": &policies.conflicts,
            },
            "provider": &provider.metadata,
            "timing": {
                "context_elapsed_ms": timings.context_elapsed_ms,
                "provider_elapsed_ms": timings.provider_elapsed_ms,
                "validation_elapsed_ms": timings.validation_elapsed_ms,
                "time_to_validated_artifact_ms": timings.time_to_validated_artifact_ms
            },
            "response_sha256": response_digest,
            "evaluation": validated,
            "validation": {
                "status": "valid",
                "aggregate_recomputed": true,
                "references_validated": true,
                "warnings": &validated.warnings,
            },
            "boundary": "Policy-guided advice is non-mutating and advisory. It cannot rewrite detector truth or change git slop check results."
        });
        let schema: Value = serde_json::from_str(include_str!("../../schemas/advice-1.json"))?;
        let validator = jsonschema::draft202012::options()
            .should_validate_formats(true)
            .build(&schema)
            .context("embedded advice artifact schema is invalid")?;
        if let Some(error) = validator.iter_errors(&artifact).next() {
            bail!(
                "generated advice artifact does not match schema v{ADVICE_SCHEMA_VERSION} at {}: {}",
                error.instance_path(),
                error
            );
        }
        let markdown = render_advice_markdown(&artifact);
        Ok(Self { artifact, markdown })
    }
}

fn string(value: Option<&Value>, fallback: &str) -> String {
    value
        .and_then(Value::as_str)
        .map(visible_controls)
        .unwrap_or_else(|| fallback.to_string())
}

fn strings(value: Option<&Value>) -> Vec<String> {
    value
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(visible_controls)
        .collect()
}

pub fn render_advice_markdown(artifact: &Value) -> String {
    let mut lines = vec![
        "# Git Slop policy-guided advice".to_string(),
        String::new(),
        format!(
            "> Aggregate verdict: **{}**. This output is advisory and cannot change detector truth or repository state.",
            string(artifact.pointer("/evaluation/aggregate_verdict"), "unknown")
        ),
        String::new(),
        string(
            artifact.pointer("/evaluation/summary"),
            "No summary supplied.",
        ),
        String::new(),
        "## Provenance".to_string(),
        String::new(),
        format!(
            "- Report: `{}`",
            string(artifact.pointer("/report/sha256"), "unknown")
        ),
        format!(
            "- Revision: `{}`",
            string(artifact.pointer("/report/head_sha"), "unknown")
        ),
        format!(
            "- Context: `{}`",
            string(artifact.pointer("/context/digest"), "unknown")
        ),
        format!(
            "- Policies: `{}`",
            string(artifact.pointer("/policies/resolution_digest"), "unknown")
        ),
        format!(
            "- Provider: `{}`",
            string(artifact.pointer("/provider/provider"), "unknown")
        ),
        format!(
            "- Model: `{}`",
            string(artifact.pointer("/provider/model"), "unknown")
        ),
        format!(
            "- Endpoint class: `{}`",
            string(
                artifact.pointer("/provider/endpoint_classification"),
                "unknown"
            )
        ),
    ];
    for candidate in artifact
        .pointer("/evaluation/candidate_evaluations")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        lines.extend([
            String::new(),
            format!(
                "## {} — {}",
                string(candidate.get("candidate_id"), "candidate"),
                string(candidate.get("aggregate_verdict"), "unknown")
            ),
            String::new(),
            string(candidate.get("rationale"), "No rationale supplied."),
            String::new(),
            "### Rule evaluations".to_string(),
            String::new(),
        ]);
        for rule in candidate
            .get("rule_evaluations")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            lines.push(format!(
                "- **{} — {}:** {}",
                string(rule.get("rule_id"), "rule"),
                string(rule.get("verdict"), "unknown"),
                string(rule.get("rationale"), "No rationale supplied.")
            ));
        }
        let revisions = strings(candidate.get("requested_revisions"));
        if !revisions.is_empty() {
            lines.extend([
                String::new(),
                "### Requested revisions".to_string(),
                String::new(),
            ]);
            lines.extend(revisions.into_iter().map(|value| format!("- {value}")));
        }
        if let Some(next) = candidate
            .get("recommended_next_step")
            .and_then(Value::as_str)
        {
            lines.extend([
                String::new(),
                "### Recommended next step".to_string(),
                String::new(),
                visible_controls(next),
            ]);
        }
        let missing = strings(candidate.get("missing_evidence"));
        if !missing.is_empty() {
            lines.extend([
                String::new(),
                "### Missing evidence".to_string(),
                String::new(),
            ]);
            lines.extend(missing.into_iter().map(|value| format!("- {value}")));
        }
    }
    let warnings = strings(artifact.pointer("/validation/warnings"));
    if !warnings.is_empty() {
        lines.extend([
            String::new(),
            "## Validation warnings".to_string(),
            String::new(),
        ]);
        lines.extend(warnings.into_iter().map(|value| format!("- {value}")));
    }
    lines.extend([
        String::new(),
        "---".to_string(),
        String::new(),
        string(artifact.get("boundary"), "Advice is advisory only."),
    ]);
    format!("{}\n", lines.join("\n"))
}

fn write_pair(directory: &Path, artifact: &Value, markdown: &str) -> Result<()> {
    fs::create_dir_all(directory)?;
    fs::write(
        directory.join("advice.json"),
        serde_json::to_string_pretty(artifact)? + "\n",
    )?;
    fs::write(directory.join("advice.md"), markdown)?;
    Ok(())
}

pub fn write_artifacts(repo_root: &Path, run: &AdviceRun) -> Result<(PathBuf, PathBuf)> {
    let root = crate::config::active_state_dir(repo_root)?.join("advice");
    let digest = run
        .artifact
        .get("response_sha256")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("advice artifact is missing response_sha256"))?;
    let generated = run
        .artifact
        .get("generated_at")
        .and_then(Value::as_str)
        .unwrap_or("unknown")
        .replace([':', '+'], "-");
    let run_id = format!("{generated}-{}", &digest[..16]);
    let runs = root.join("runs");
    fs::create_dir_all(&runs)?;
    let run_dir = runs.join(&run_id);
    if run_dir.exists() {
        bail!("advice run already exists: {}", run_dir.display());
    }
    let temporary_run = runs.join(format!(".{run_id}-{}.tmp", std::process::id()));
    write_pair(&temporary_run, &run.artifact, &run.markdown)?;
    fs::rename(&temporary_run, &run_dir)?;

    let latest = root.join("latest");
    let temporary_latest = root.join(format!(".latest-{}.tmp", std::process::id()));
    if temporary_latest.exists() {
        fs::remove_dir_all(&temporary_latest)?;
    }
    write_pair(&temporary_latest, &run.artifact, &run.markdown)?;
    let backup = root.join(format!(".latest-{}.backup", std::process::id()));
    if latest.exists() {
        fs::rename(&latest, &backup)?;
    }
    if let Err(error) = fs::rename(&temporary_latest, &latest) {
        let _ = fs::remove_dir_all(&temporary_latest);
        if backup.exists() {
            let _ = fs::rename(&backup, &latest);
        }
        return Err(error.into());
    }
    if backup.exists() {
        fs::remove_dir_all(backup)?;
    }
    Ok((latest.join("advice.json"), latest.join("advice.md")))
}

pub fn load_and_validate_artifact(path: &Path, report: &Value) -> Result<Value> {
    let artifact: Value = serde_json::from_slice(&fs::read(path)?)
        .with_context(|| format!("unable to parse advice artifact {}", path.display()))?;
    let schema: Value = serde_json::from_str(include_str!("../../schemas/advice-1.json"))?;
    let validator = jsonschema::draft202012::options()
        .should_validate_formats(true)
        .build(&schema)
        .context("embedded advice artifact schema is invalid")?;
    if let Some(error) = validator.iter_errors(&artifact).next() {
        bail!(
            "advice artifact does not match schema v{ADVICE_SCHEMA_VERSION} at {}: {}",
            error.instance_path(),
            error
        );
    }
    if artifact.get("schema_version").and_then(Value::as_u64) != Some(ADVICE_SCHEMA_VERSION)
        || artifact
            .pointer("/validation/status")
            .and_then(Value::as_str)
            != Some("valid")
    {
        bail!("advice artifact is not a validated schema-{ADVICE_SCHEMA_VERSION} artifact");
    }
    let current = sha256(serde_json::to_vec(report)?);
    let recorded = artifact
        .pointer("/report/canonical_sha256")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if current != recorded {
        bail!(
            "stale advice artifact: recorded report digest {recorded}, current selected report digest {current}"
        );
    }
    Ok(artifact)
}
