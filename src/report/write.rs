use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering as AtomicOrdering};

use anyhow::{Context, Result, anyhow};
use chrono::{DateTime, Utc};
use serde_json::{Value, json};

use super::assembly::assemble_report;
use super::render::{render_compatibility_summary, render_terminal};
use crate::config;
use crate::health::render_health_from_report;
use crate::model::{
    Analysis, FileAnalysis, FindResult, FolderAnalysis, HealthRollup, ScopeIdentity,
};

static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

pub fn schema() -> Value {
    json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "$id": "https://github.com/coreycoto/git-slop/blob/main/schemas/report-4.json",
        "title": "Git Slop report schema 4",
        "type": "object",
        "additionalProperties": true,
        "required": ["schema_version", "analyzer", "generated_at", "repo", "scope", "config", "stats", "summary", "files", "folders", "action_queue", "overlays", "health", "diagnostics"],
        "properties": {
            "schema_version": {"const": 4},
            "analyzer": {
                "type": "object",
                "additionalProperties": false,
                "required": ["name", "version", "config_digest", "context_tokenizer"],
                "properties": {
                    "name": {"const": "git-slop"},
                    "version": {"type": "string"},
                    "config_digest": {"type": "string", "pattern": "^[a-f0-9]{64}$"},
                    "context_tokenizer": {"type": "string"}
                }
            },
            "repo": {"type": "object", "required": ["repo_name", "worktree_state_digest", "analyzed_content_digest"]},
            "scope": {
                "type": "object",
                "additionalProperties": false,
                "required": ["mode", "path", "selected_path_count", "selected_path_digest"],
                "properties": {
                    "mode": {"enum": ["repository", "scoped"]},
                    "path": {"type": ["string", "null"]},
                    "selected_path_count": {"type": "integer", "minimum": 0},
                    "selected_path_digest": {"type": "string", "pattern": "^[a-f0-9]{64}$"}
                }
            },
            "config": {"type": "object"},
            "stats": {"type": "object"},
            "summary": {"type": "object"},
            "files": {"type": "array", "items": {"$ref": "#/$defs/file"}},
            "folders": {"type": "array", "items": {"$ref": "#/$defs/folder"}},
            "action_queue": {"type": "array", "items": {"type": "object", "required": ["path"]}},
            "overlays": {"type": "object"},
            "health": {"type": "object"},
            "diagnostics": {"type": "object"}
        },
        "$defs": {
            "file": {"type": "object", "required": ["path", "profile", "classification", "tokens", "context_band", "content_fingerprint", "slop_score", "slop_band", "reason_codes", "costs", "overlays"]},
            "folder": {"type": "object", "required": ["path", "tokens", "context_band", "slop_score", "slop_band", "reason_codes", "costs", "overlays"]}
        }
    })
}

pub fn load_report(path: &Path) -> Result<Value> {
    let source =
        fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;
    let report: Value = serde_json::from_str(&source)
        .with_context(|| format!("invalid git-slop report JSON: {}", path.display()))?;
    validate_report_shape(&report)
        .with_context(|| format!("invalid git-slop report shape: {}", path.display()))?;
    Ok(report)
}

pub fn validate_report_shape(report: &Value) -> Result<()> {
    let Some(root) = report.as_object() else {
        anyhow::bail!("report root must be an object");
    };
    if root.get("schema_version").and_then(Value::as_u64) != Some(4) {
        anyhow::bail!("schema_version must be 4");
    }
    for key in ["repo", "config", "stats", "summary", "overlays", "health"] {
        if !root.get(key).is_some_and(Value::is_object) {
            anyhow::bail!("{key} must be an object");
        }
    }
    let repo = root["repo"].as_object().expect("repo checked as object");
    if repo.get("repo_name").and_then(Value::as_str).is_none() {
        anyhow::bail!("repo.repo_name must be a string");
    }
    for key in ["files", "folders", "action_queue"] {
        let Some(records) = root.get(key).and_then(Value::as_array) else {
            anyhow::bail!("{key} must be an array");
        };
        for (index, record) in records.iter().enumerate() {
            if !record.is_object() {
                anyhow::bail!("{key}[{index}] must be an object");
            }
            if record.get("path").and_then(Value::as_str).is_none() {
                anyhow::bail!("{key}[{index}].path must be a string");
            }
        }
    }
    let canonical = root.contains_key("analyzer");
    for (index, record) in root["files"]
        .as_array()
        .expect("files checked as array")
        .iter()
        .enumerate()
    {
        if !canonical {
            continue;
        }
        for (field, kind) in [
            ("tokens", "integer"),
            ("context_band", "string"),
            ("slop_score", "number"),
            ("slop_band", "string"),
            ("reason_codes", "array"),
            ("costs", "object"),
            ("overlays", "object"),
        ] {
            let Some(value) = record.get(field) else {
                anyhow::bail!("files[{index}].{field} is required by report schema 4");
            };
            let valid = match kind {
                "integer" => value.as_u64().is_some(),
                "number" => value.as_f64().is_some_and(f64::is_finite),
                "string" => value.is_string(),
                "array" => value.is_array(),
                "object" => value.is_object(),
                _ => false,
            };
            if !valid {
                anyhow::bail!("files[{index}].{field} must be a finite {kind}");
            }
        }
    }

    // Reports emitted by the current analyzer carry the complete canonical
    // contract. Historical synthetic schema-4 fixtures without analyzer
    // metadata remain readable as migration inputs, but cannot masquerade as
    // a freshly generated exhaustive report.
    if canonical {
        for key in [
            "generated_at",
            "analyzed_revision_at",
            "scope",
            "evidence_completeness",
            "diagnostics",
            "costs",
            "organization_metrics",
            "relationships",
            "clusters",
        ] {
            if !root.contains_key(key) {
                anyhow::bail!("{key} is required in canonical schema-4 reports");
            }
        }
        let analyzer = root["analyzer"]
            .as_object()
            .ok_or_else(|| anyhow!("analyzer must be an object"))?;
        for key in ["name", "version", "config_digest", "context_tokenizer"] {
            if analyzer.get(key).and_then(Value::as_str).is_none() {
                anyhow::bail!("analyzer.{key} must be a string");
            }
        }
        serde_json::from_value::<ScopeIdentity>(root["scope"].clone())
            .context("scope does not match the canonical schema-4 contract")?;
        crate::config::validate(&root["config"])
            .context("embedded effective configuration is invalid")?;
        for (index, file) in root["files"]
            .as_array()
            .expect("files checked as array")
            .iter()
            .enumerate()
        {
            serde_json::from_value::<FileAnalysis>(file.clone()).with_context(|| {
                format!("files[{index}] does not match the canonical schema-4 contract")
            })?;
        }
        for (index, folder) in root["folders"]
            .as_array()
            .expect("folders checked as array")
            .iter()
            .enumerate()
        {
            serde_json::from_value::<FolderAnalysis>(folder.clone()).with_context(|| {
                format!("folders[{index}] does not match the canonical schema-4 contract")
            })?;
        }
        serde_json::from_value::<HealthRollup>(root["health"].clone())
            .context("health does not match the canonical schema-4 contract")?;
    }
    Ok(())
}

fn timestamp_slug(generated_at: &str) -> String {
    if let Ok(timestamp) = DateTime::parse_from_rfc3339(generated_at) {
        return timestamp
            .with_timezone(&Utc)
            .format("%Y%m%dT%H%M%SZ")
            .to_string();
    }
    let normalized = generated_at
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character
            } else {
                '-'
            }
        })
        .collect::<String>();
    let normalized = normalized.trim_matches('-');
    if normalized.is_empty() {
        Utc::now().format("%Y%m%dT%H%M%SZ").to_string()
    } else {
        normalized.to_string()
    }
}

fn temporary_directory(parent: &Path, label: &str) -> PathBuf {
    let sequence = TEMP_SEQUENCE.fetch_add(1, AtomicOrdering::Relaxed);
    parent.join(format!(".{label}-{}-{sequence}.tmp", std::process::id()))
}

fn unique_run_root(runs_root: &Path, preferred_slug: &str) -> PathBuf {
    let preferred = runs_root.join(preferred_slug);
    if !preferred.exists() {
        return preferred;
    }
    for suffix in 2..10_000 {
        let candidate = runs_root.join(format!("{preferred_slug}-{suffix}"));
        if !candidate.exists() {
            return candidate;
        }
    }
    runs_root.join(format!(
        "{preferred_slug}-{}",
        TEMP_SEQUENCE.fetch_add(1, AtomicOrdering::Relaxed)
    ))
}

fn write_bundle_files(
    root: &Path,
    report_json: &str,
    report_yaml: Option<&str>,
    summary: &str,
    health: &str,
) -> Result<()> {
    fs::create_dir_all(root)
        .with_context(|| format!("failed to create report directory {}", root.display()))?;
    let mut files = vec![
        ("report.json", report_json),
        ("summary.md", summary),
        ("health.md", health),
    ];
    if let Some(report_yaml) = report_yaml {
        files.push(("report.yaml", report_yaml));
    }
    for (name, content) in files {
        let path = root.join(name);
        fs::write(&path, content).with_context(|| format!("failed to write {}", path.display()))?;
    }
    Ok(())
}

fn replace_latest_from_run(latest: &Path, run_root: &Path, yaml_enabled: bool) -> Result<()> {
    let parent = latest.parent().ok_or_else(|| {
        anyhow!(
            "latest report directory has no parent: {}",
            latest.display()
        )
    })?;
    let temporary = temporary_directory(parent, "latest");
    let backup = temporary_directory(parent, "latest-backup");
    fs::create_dir_all(&temporary)?;
    let mut names = vec!["report.json", "summary.md", "health.md"];
    if yaml_enabled {
        names.push("report.yaml");
    }
    for name in names {
        let source = run_root.join(name);
        let target = temporary.join(name);
        if fs::hard_link(&source, &target).is_err() {
            fs::copy(&source, &target)
                .with_context(|| format!("failed to materialize {}", target.display()))?;
        }
    }
    if latest.exists() {
        fs::rename(latest, &backup)?;
    }
    if let Err(error) = fs::rename(&temporary, latest) {
        if backup.exists() && !latest.exists() {
            let _ = fs::rename(&backup, latest);
        }
        return Err(error).with_context(|| {
            format!(
                "failed to publish latest report directory {}",
                latest.display()
            )
        });
    }
    if backup.exists() {
        fs::remove_dir_all(backup)?;
    }
    Ok(())
}

fn enforce_retention(runs_root: &Path, keep: usize) -> Result<()> {
    let mut runs = fs::read_dir(runs_root)?
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
        .collect::<Vec<_>>();
    runs.sort_by_key(|entry| std::cmp::Reverse(entry.file_name()));
    for entry in runs.into_iter().skip(keep) {
        fs::remove_dir_all(entry.path())?;
    }
    Ok(())
}

fn cleanup_abandoned_publication_state(slop_root: &Path) -> Vec<String> {
    let mut warnings = Vec::new();
    let latest = slop_root.join("latest");
    let mut backups = Vec::new();
    let Ok(entries) = fs::read_dir(slop_root) else {
        return warnings;
    };
    for entry in entries.filter_map(Result::ok) {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with(".latest-backup-") && name.ends_with(".tmp") {
            backups.push(entry.path());
        } else if (name.starts_with(".latest-") || name.starts_with(".run-"))
            && name.ends_with(".tmp")
        {
            if let Err(error) = fs::remove_dir_all(entry.path()) {
                warnings.push(format!(
                    "failed to remove abandoned publication temporary {}: {error}",
                    entry.path().display()
                ));
            }
        }
    }
    backups.sort();
    if !latest.exists() {
        if let Some(recovery) = backups.pop() {
            if let Err(error) = fs::rename(&recovery, &latest) {
                warnings.push(format!(
                    "failed to recover latest report from {}: {error}",
                    recovery.display()
                ));
            }
        }
    }
    for backup in backups {
        if let Err(error) = fs::remove_dir_all(&backup) {
            warnings.push(format!(
                "failed to remove abandoned latest backup {}: {error}",
                backup.display()
            ));
        }
    }
    if let Ok(entries) = fs::read_dir(slop_root.join("runs")) {
        for entry in entries.filter_map(Result::ok) {
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with(".run-") && name.ends_with(".tmp") {
                if let Err(error) = fs::remove_dir_all(entry.path()) {
                    warnings.push(format!(
                        "failed to remove abandoned run temporary {}: {error}",
                        entry.path().display()
                    ));
                }
            }
        }
    }
    warnings
}

fn write_run_atomically(
    run_root: &Path,
    report_json: &str,
    report_yaml: Option<&str>,
    summary: &str,
    health: &str,
) -> Result<()> {
    let parent = run_root
        .parent()
        .ok_or_else(|| anyhow!("run report directory has no parent: {}", run_root.display()))?;
    let temporary = temporary_directory(parent, "run");
    if let Err(error) = write_bundle_files(&temporary, report_json, report_yaml, summary, health) {
        let _ = fs::remove_dir_all(&temporary);
        return Err(error);
    }
    if let Err(error) = fs::rename(&temporary, run_root) {
        let _ = fs::remove_dir_all(&temporary);
        return Err(error).with_context(|| {
            format!(
                "failed to publish timestamped report directory {}",
                run_root.display()
            )
        });
    }
    Ok(())
}

pub fn write_report_bundle(analysis: &Analysis, health: &HealthRollup) -> Result<FindResult> {
    config::ensure_state_dirs(&analysis.repo_root)?;
    let runs_root = config::runs_dir(&analysis.repo_root);
    fs::create_dir_all(&runs_root)
        .with_context(|| format!("failed to create {}", runs_root.display()))?;
    let retention = config::pointer_u64(&analysis.config, "/output/retention_runs", 20) as usize;
    let mut warnings = cleanup_abandoned_publication_state(&config::slop_dir(&analysis.repo_root));
    let retention_warning = enforce_retention(&runs_root, retention.saturating_sub(1))
        .err()
        .map(|error| format!("old report retention could not be completed: {error:#}"));
    warnings.extend(retention_warning);
    let mut report = assemble_report(analysis, health);
    if !warnings.is_empty() {
        report["diagnostics"]["warnings"] = json!(warnings);
    }
    let pretty_json = config::pointer_bool(&analysis.config, "/output/pretty_json", false);
    let yaml_enabled = config::pointer_bool(&analysis.config, "/output/yaml", false);
    let mut previous_sizes = (0usize, 0usize);
    for _ in 0..4 {
        let json_bytes = if pretty_json {
            serde_json::to_string_pretty(&report)?.len() + 1
        } else {
            serde_json::to_string(&report)?.len() + 1
        };
        let yaml_bytes = if yaml_enabled {
            serde_yaml::to_string(&report)?.len()
        } else {
            0
        };
        if (json_bytes, yaml_bytes) == previous_sizes {
            break;
        }
        previous_sizes = (json_bytes, yaml_bytes);
        report["diagnostics"]["report_sizes"] = json!({
            "report_json_bytes": json_bytes,
            "report_yaml_bytes": yaml_bytes,
        });
    }
    let report_json = if pretty_json {
        serde_json::to_string_pretty(&report)
    } else {
        serde_json::to_string(&report)
    }
    .context("failed to render report JSON")?
        + "\n";
    let report_yaml = yaml_enabled
        .then(|| serde_yaml::to_string(&report).context("failed to render report YAML"))
        .transpose()?;
    let summary = render_compatibility_summary(&report);
    let health_markdown = render_health_from_report(&report)?;
    let terminal = render_terminal(&report);

    let run_root = unique_run_root(&runs_root, &timestamp_slug(&analysis.generated_at));
    write_run_atomically(
        &run_root,
        &report_json,
        report_yaml.as_deref(),
        &summary,
        &health_markdown,
    )?;
    let latest = config::latest_dir(&analysis.repo_root);
    replace_latest_from_run(&latest, &run_root, yaml_enabled)?;

    Ok(FindResult {
        report,
        report_json: latest.join("report.json"),
        report_yaml: latest.join("report.yaml"),
        summary_md: latest.join("summary.md"),
        health_md: latest.join("health.md"),
        terminal,
    })
}
