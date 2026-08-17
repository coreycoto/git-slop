use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::Write;
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

pub(super) fn ensure_private_directory(directory: &Path) -> Result<()> {
    if directory
        .symlink_metadata()
        .is_ok_and(|metadata| metadata.file_type().is_symlink())
    {
        bail!(
            "advice state directory must not be a symbolic link: {}",
            directory.display()
        );
    }
    fs::create_dir_all(directory)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(directory, fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

fn write_private_file(path: &Path, bytes: &[u8]) -> Result<()> {
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path)?;
    if let Err(error) = (|| -> std::io::Result<()> {
        file.write_all(bytes)?;
        file.sync_all()
    })() {
        let _ = fs::remove_file(path);
        return Err(error.into());
    }
    Ok(())
}

pub(super) fn sync_directory(path: &Path) -> Result<()> {
    #[cfg(unix)]
    fs::File::open(path)?.sync_all()?;
    Ok(())
}

fn write_pair(directory: &Path, artifact: &Value, markdown: &str) -> Result<()> {
    ensure_private_directory(directory)?;
    let json = serde_json::to_string_pretty(artifact)? + "\n";
    write_private_file(&directory.join("advice.json"), json.as_bytes())?;
    if let Err(error) = write_private_file(&directory.join("advice.md"), markdown.as_bytes()) {
        let _ = fs::remove_file(directory.join("advice.json"));
        return Err(error);
    }
    sync_directory(directory)?;
    Ok(())
}

fn remove_stale_advice_temporaries(directory: &Path, prefix: &str) -> Result<()> {
    if !directory.exists() {
        return Ok(());
    }
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        if entry
            .file_name()
            .to_str()
            .is_some_and(|name| name.starts_with(prefix))
        {
            let metadata = fs::symlink_metadata(entry.path())?;
            if metadata.is_dir() {
                fs::remove_dir_all(entry.path())?;
            } else {
                fs::remove_file(entry.path())?;
            }
        }
    }
    Ok(())
}

fn recover_advice_state(root: &Path, runs: &Path) -> Result<()> {
    let latest = root.join("latest");
    let backup = root.join(".latest.backup");
    if backup.exists() {
        if latest.exists() {
            fs::remove_dir_all(&backup)?;
        } else {
            fs::rename(&backup, &latest)?;
        }
    }
    remove_stale_advice_temporaries(root, ".advice-latest-")?;
    remove_stale_advice_temporaries(runs, ".advice-run-")?;
    sync_directory(root)?;
    sync_directory(runs)?;
    Ok(())
}

pub fn write_artifacts(repo_root: &Path, run: &AdviceRun) -> Result<(PathBuf, PathBuf)> {
    let root = crate::config::active_state_dir(repo_root)?.join("advice");
    ensure_private_directory(&root)?;
    let lock_path = root.join(".write.lock");
    let mut lock_options = fs::OpenOptions::new();
    lock_options.read(true).write(true).create(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        lock_options.mode(0o600);
    }
    let lock = lock_options.open(&lock_path)?;
    fs2::FileExt::lock_exclusive(&lock)?;
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
    ensure_private_directory(&runs)?;
    recover_advice_state(&root, &runs)?;
    let run_dir = runs.join(&run_id);
    if run_dir.exists() {
        bail!("advice run already exists: {}", run_dir.display());
    }
    let temporary_run = tempfile::Builder::new()
        .prefix(".advice-run-")
        .tempdir_in(&runs)?;
    write_pair(temporary_run.path(), &run.artifact, &run.markdown)?;
    fs::rename(temporary_run.path(), &run_dir)?;
    sync_directory(&runs)?;

    let latest = root.join("latest");
    let temporary_latest = tempfile::Builder::new()
        .prefix(".advice-latest-")
        .tempdir_in(&root)?;
    write_pair(temporary_latest.path(), &run.artifact, &run.markdown)?;
    let backup = root.join(".latest.backup");
    if latest.exists() {
        fs::rename(&latest, &backup)?;
        sync_directory(&root)?;
    }
    if let Err(error) = fs::rename(temporary_latest.path(), &latest) {
        if backup.exists() {
            let _ = fs::rename(&backup, &latest);
        }
        let _ = sync_directory(&root);
        return Err(error.into());
    }
    sync_directory(&root)?;
    if backup.exists() {
        fs::remove_dir_all(&backup)?;
        sync_directory(&root)?;
    }
    Ok((latest.join("advice.json"), latest.join("advice.md")))
}

pub fn load_and_validate_artifact(path: &Path, report: &Value) -> Result<Value> {
    let bytes = super::io::read_bounded(
        path,
        super::io::MAX_ADVICE_ARTIFACT_BYTES,
        "advice artifact",
    )?;
    let artifact: Value = serde_json::from_slice(&bytes)
        .with_context(|| format!("unable to parse advice artifact {}", path.display()))?;
    validate_artifact_contract(&artifact)?;
    validate_artifact_semantics(&artifact)?;
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

fn validate_artifact_contract(artifact: &Value) -> Result<()> {
    let schema: Value = serde_json::from_str(include_str!("../../schemas/advice-1.json"))?;
    let validator = jsonschema::draft202012::options()
        .should_validate_formats(true)
        .build(&schema)
        .context("embedded advice artifact schema is invalid")?;
    if let Some(error) = validator.iter_errors(artifact).next() {
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
    Ok(())
}

fn artifact_verdict_rank(value: &str) -> Option<u8> {
    match value {
        "approve" => Some(0),
        "abstain" => Some(1),
        "revise" => Some(2),
        "reject" => Some(3),
        _ => None,
    }
}

fn aggregate_artifact_verdict<'a>(
    verdicts: impl IntoIterator<Item = &'a str>,
) -> Option<&'static str> {
    let rank = verdicts
        .into_iter()
        .filter_map(artifact_verdict_rank)
        .max()?;
    Some(match rank {
        0 => "approve",
        1 => "abstain",
        2 => "revise",
        _ => "reject",
    })
}

fn artifact_reference_sets(artifact: &Value) -> BTreeMap<&'static str, BTreeSet<&str>> {
    [
        "candidates",
        "paths",
        "findings",
        "relationships",
        "clusters",
        "excerpts",
        "policies",
        "verification",
    ]
    .into_iter()
    .map(|category| {
        let values = artifact
            .pointer(&format!("/context/reference_index/{category}"))
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .collect();
        (category, values)
    })
    .collect()
}

fn validate_artifact_citations(
    citations: &Value,
    references: &BTreeMap<&str, BTreeSet<&str>>,
) -> Result<()> {
    let mut count = 0_usize;
    for (category, available) in references {
        let supplied = citations
            .get(*category)
            .and_then(Value::as_array)
            .ok_or_else(|| anyhow::anyhow!("advice artifact citation set is incomplete"))?;
        count = count.saturating_add(supplied.len());
        for reference in supplied.iter().filter_map(Value::as_str) {
            if !available.contains(reference) {
                bail!(
                    "advice artifact contains an invented or unavailable {category} citation {reference:?}"
                );
            }
        }
    }
    if count == 0 {
        bail!("advice artifact rationale has no supplied evidence citation");
    }
    Ok(())
}

fn validate_artifact_semantics(artifact: &Value) -> Result<()> {
    let references = artifact_reference_sets(artifact);
    let expected_candidates = &references["candidates"];
    let candidate_ids = artifact
        .get("candidate_ids")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect::<BTreeSet<_>>();
    if &candidate_ids != expected_candidates {
        bail!("advice artifact candidate identity drifted from its reference index");
    }
    let candidates = artifact
        .pointer("/evaluation/candidate_evaluations")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow::anyhow!("advice artifact has no candidate evaluations"))?;
    let mut observed_candidates = BTreeSet::new();
    let mut candidate_verdicts = Vec::new();
    for candidate in candidates {
        let candidate_id = candidate
            .get("candidate_id")
            .and_then(Value::as_str)
            .ok_or_else(|| anyhow::anyhow!("advice artifact candidate identity is missing"))?;
        if !expected_candidates.contains(candidate_id) || !observed_candidates.insert(candidate_id)
        {
            bail!("advice artifact contains an unknown or duplicate candidate {candidate_id}");
        }
        validate_artifact_citations(&candidate["citations"], &references)?;
        let rules = candidate["rule_evaluations"]
            .as_array()
            .ok_or_else(|| anyhow::anyhow!("advice artifact rule evaluations are missing"))?;
        let mut observed_rules = BTreeSet::new();
        for rule in rules {
            let rule_id = rule["rule_id"]
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("advice artifact rule identity is missing"))?;
            if !references["policies"].contains(rule_id) || !observed_rules.insert(rule_id) {
                bail!("advice artifact contains an unknown or duplicate policy rule {rule_id}");
            }
            validate_artifact_citations(&rule["citations"], &references)?;
            if !rule["citations"]["policies"]
                .as_array()
                .is_some_and(|values| values.iter().any(|value| value.as_str() == Some(rule_id)))
            {
                bail!("advice artifact rule {rule_id} does not cite its supplied policy ID");
            }
        }
        if observed_rules != references["policies"] {
            bail!("advice artifact candidate {candidate_id} has an incomplete policy matrix");
        }
        let computed =
            aggregate_artifact_verdict(rules.iter().filter_map(|rule| rule["verdict"].as_str()))
                .ok_or_else(|| anyhow::anyhow!("advice artifact has no policy verdicts"))?;
        if candidate["aggregate_verdict"].as_str() != Some(computed) {
            bail!("advice artifact candidate {candidate_id} has a stale aggregate verdict");
        }
        candidate_verdicts.push(computed);
    }
    if &observed_candidates != expected_candidates {
        bail!("advice artifact is missing one or more candidate evaluations");
    }
    let computed = aggregate_artifact_verdict(candidate_verdicts)
        .ok_or_else(|| anyhow::anyhow!("advice artifact has no aggregate verdict evidence"))?;
    if artifact
        .pointer("/evaluation/aggregate_verdict")
        .and_then(Value::as_str)
        != Some(computed)
    {
        bail!("advice artifact has a stale recomputed aggregate verdict");
    }
    if artifact.pointer("/evaluation/warnings") != artifact.pointer("/validation/warnings") {
        bail!("advice artifact validation warnings drifted from evaluation evidence");
    }
    Ok(())
}

fn advice_directory_size(path: &Path) -> Result<u64> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() {
        bail!(
            "advice state must not contain symbolic links: {}",
            path.display()
        );
    }
    if metadata.is_file() {
        return Ok(metadata.len());
    }
    let mut total = 0_u64;
    for entry in fs::read_dir(path)? {
        total = total.saturating_add(advice_directory_size(&entry?.path())?);
    }
    Ok(total)
}

#[cfg(unix)]
fn advice_permissions_private(path: &Path) -> Result<bool> {
    use std::os::unix::fs::PermissionsExt;

    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() || metadata.permissions().mode() & 0o077 != 0 {
        return Ok(false);
    }
    if metadata.is_dir() {
        for entry in fs::read_dir(path)? {
            if !advice_permissions_private(&entry?.path())? {
                return Ok(false);
            }
        }
    }
    Ok(true)
}

#[cfg(not(unix))]
fn advice_permissions_private(_path: &Path) -> Result<bool> {
    Ok(true)
}

pub fn state_status(state_root: &Path, current_report: Option<&Value>) -> Result<Value> {
    let root = state_root.join("advice");
    if !root.exists() {
        return Ok(json!({
            "status": "missing",
            "latest": "missing",
            "retained_runs": 0,
            "retained_bytes": 0,
            "private_permissions": true,
            "recovery_entries": 0,
            "retention_command": "git slop prune --dry-run"
        }));
    }
    if root
        .symlink_metadata()
        .is_ok_and(|metadata| metadata.file_type().is_symlink())
    {
        bail!("advice state root must not be a symbolic link");
    }
    let recovery_entries = fs::read_dir(&root)?
        .filter_map(Result::ok)
        .filter(|entry| {
            entry
                .file_name()
                .to_str()
                .is_some_and(|name| name == ".latest.backup" || name.starts_with(".advice-latest-"))
        })
        .count();
    let runs = root.join("runs");
    let retained = if runs.is_dir() {
        fs::read_dir(&runs)?
            .filter_map(Result::ok)
            .filter(|entry| {
                entry.file_type().is_ok_and(|kind| kind.is_dir())
                    && entry
                        .file_name()
                        .to_str()
                        .is_some_and(|name| !name.starts_with('.'))
            })
            .map(|entry| {
                let bytes = advice_directory_size(&entry.path())?;
                Ok(bytes)
            })
            .collect::<Result<Vec<_>>>()?
    } else {
        Vec::new()
    };
    let latest_path = root.join("latest/advice.json");
    let markdown_path = root.join("latest/advice.md");
    let latest = if !latest_path.exists() && !markdown_path.exists() {
        "missing"
    } else if !latest_path.is_file() || !markdown_path.is_file() {
        "invalid"
    } else {
        let bytes = super::io::read_bounded(
            &latest_path,
            super::io::MAX_ADVICE_ARTIFACT_BYTES,
            "latest advice artifact",
        );
        bytes
            .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).map_err(Into::into))
            .and_then(|artifact| {
                validate_artifact_contract(&artifact)?;
                validate_artifact_semantics(&artifact)?;
                if let Some(report) = current_report {
                    let current = sha256(serde_json::to_vec(report)?);
                    if artifact
                        .pointer("/report/canonical_sha256")
                        .and_then(Value::as_str)
                        != Some(current.as_str())
                    {
                        return Ok("stale");
                    }
                }
                Ok("valid")
            })
            .unwrap_or("invalid")
    };
    let private_permissions = advice_permissions_private(&root)?;
    let status = if recovery_entries > 0 {
        "recovery_required"
    } else if !private_permissions {
        "insecure_permissions"
    } else {
        latest
    };
    Ok(json!({
        "status": status,
        "latest": latest,
        "retained_runs": retained.len(),
        "retained_bytes": retained.into_iter().sum::<u64>(),
        "private_permissions": private_permissions,
        "recovery_entries": recovery_entries,
        "retention_command": "git slop prune --dry-run"
    }))
}
