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
use crate::model::{Analysis, FindResult, HealthRollup};

static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

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
    report_yaml: &str,
    summary: &str,
    health: &str,
) -> Result<()> {
    fs::create_dir_all(root)
        .with_context(|| format!("failed to create report directory {}", root.display()))?;
    for (name, content) in [
        ("report.json", report_json),
        ("report.yaml", report_yaml),
        ("summary.md", summary),
        ("health.md", health),
    ] {
        let path = root.join(name);
        fs::write(&path, content).with_context(|| format!("failed to write {}", path.display()))?;
    }
    Ok(())
}

fn replace_latest_from_run(latest: &Path, run_root: &Path) -> Result<()> {
    let parent = latest.parent().ok_or_else(|| {
        anyhow!(
            "latest report directory has no parent: {}",
            latest.display()
        )
    })?;
    let temporary = temporary_directory(parent, "latest");
    let backup = temporary_directory(parent, "latest-backup");
    fs::create_dir_all(&temporary)?;
    for name in ["report.json", "report.yaml", "summary.md", "health.md"] {
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

fn write_run_atomically(
    run_root: &Path,
    report_json: &str,
    report_yaml: &str,
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
    let mut report = assemble_report(analysis, health);
    let mut previous_sizes = (0usize, 0usize);
    for _ in 0..4 {
        let json_bytes = serde_json::to_string_pretty(&report)?.len() + 1;
        let yaml_bytes = serde_yaml::to_string(&report)?.len();
        if (json_bytes, yaml_bytes) == previous_sizes {
            break;
        }
        previous_sizes = (json_bytes, yaml_bytes);
        report["diagnostics"]["report_sizes"] = json!({
            "report_json_bytes": json_bytes,
            "report_yaml_bytes": yaml_bytes,
        });
    }
    let report_json =
        serde_json::to_string_pretty(&report).context("failed to render report JSON")? + "\n";
    let report_yaml = serde_yaml::to_string(&report).context("failed to render report YAML")?;
    let summary = render_compatibility_summary(&report);
    let health_markdown = render_health_from_report(&report)?;
    let terminal = render_terminal(&report);

    let runs_root = config::runs_dir(&analysis.repo_root);
    fs::create_dir_all(&runs_root)
        .with_context(|| format!("failed to create {}", runs_root.display()))?;
    let run_root = unique_run_root(&runs_root, &timestamp_slug(&analysis.generated_at));
    write_run_atomically(
        &run_root,
        &report_json,
        &report_yaml,
        &summary,
        &health_markdown,
    )?;
    let latest = config::latest_dir(&analysis.repo_root);
    replace_latest_from_run(&latest, &run_root)?;
    let retention = config::pointer_u64(&analysis.config, "/output/retention_runs", 20) as usize;
    enforce_retention(&runs_root, retention)?;

    Ok(FindResult {
        report,
        report_json: latest.join("report.json"),
        report_yaml: latest.join("report.yaml"),
        summary_md: latest.join("summary.md"),
        health_md: latest.join("health.md"),
        terminal,
    })
}
