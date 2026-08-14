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
    compressed: Option<(&str, &[u8])>,
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
    if let Some((name, bytes)) = compressed {
        let path = root.join(name);
        fs::write(&path, bytes).with_context(|| format!("failed to write {}", path.display()))?;
    }
    Ok(())
}

fn replace_latest_from_run(
    latest: &Path,
    run_root: &Path,
    yaml_enabled: bool,
    compressed_name: Option<&str>,
) -> Result<()> {
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
    if let Some(name) = compressed_name {
        names.push(name);
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

fn retained_directory_size(path: &Path) -> Result<u64> {
    let mut bytes = 0u64;
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let metadata = fs::symlink_metadata(entry.path())?;
        bytes = bytes.saturating_add(if metadata.is_dir() {
            retained_directory_size(&entry.path())?
        } else {
            metadata.len()
        });
    }
    Ok(bytes)
}

fn enforce_retention(runs_root: &Path, keep: usize, max_bytes: u64) -> Result<()> {
    let mut runs = fs::read_dir(runs_root)?
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
        .map(|entry| {
            let bytes = retained_directory_size(&entry.path())?;
            Ok((entry, bytes))
        })
        .collect::<Result<Vec<_>>>()?;
    runs.sort_by_key(|(entry, _)| std::cmp::Reverse(entry.file_name()));
    let mut retained_bytes = 0u64;
    for (index, (entry, bytes)) in runs.into_iter().enumerate() {
        let retain_newest_even_if_oversized = index == 0 && keep > 0;
        if retain_newest_even_if_oversized
            || (index < keep && retained_bytes.saturating_add(bytes) <= max_bytes)
        {
            retained_bytes = retained_bytes.saturating_add(bytes);
            continue;
        }
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
    compressed: Option<(&str, &[u8])>,
) -> Result<()> {
    let parent = run_root
        .parent()
        .ok_or_else(|| anyhow!("run report directory has no parent: {}", run_root.display()))?;
    let temporary = temporary_directory(parent, "run");
    if let Err(error) = write_bundle_files(
        &temporary,
        report_json,
        report_yaml,
        summary,
        health,
        compressed,
    ) {
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
    fs::create_dir_all(&analysis.output_root)
        .with_context(|| format!("failed to create {}", analysis.output_root.display()))?;
    let runs_root = analysis.output_root.join("runs");
    fs::create_dir_all(&runs_root)
        .with_context(|| format!("failed to create {}", runs_root.display()))?;
    let retention = config::pointer_u64(&analysis.config, "/output/retention_runs", 20) as usize;
    let retention_bytes =
        config::pointer_u64(&analysis.config, "/output/retention_bytes", 2_147_483_648);
    let mut warnings = cleanup_abandoned_publication_state(&analysis.output_root);
    let retention_warning =
        enforce_retention(&runs_root, retention.saturating_sub(1), retention_bytes)
            .err()
            .map(|error| format!("old report retention could not be completed: {error:#}"));
    warnings.extend(retention_warning);
    let mut report = assemble_report(analysis, health);
    apply_report_profile(&mut report, &analysis.report_profile);
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
            "logical_artifact_bytes": json_bytes.saturating_add(yaml_bytes),
            "physical_storage_semantics": "latest may hard-link immutable run artifacts; do not sum logical paths to estimate allocated disk bytes",
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
    let compressed = compressed_bytes(&analysis.compression, report_json.as_bytes())?;

    let run_root = unique_run_root(&runs_root, &timestamp_slug(&analysis.generated_at));
    write_run_atomically(
        &run_root,
        &report_json,
        report_yaml.as_deref(),
        &summary,
        &health_markdown,
        compressed
            .as_ref()
            .map(|(name, bytes)| (name.as_str(), bytes.as_slice())),
    )?;
    let newest_run_bytes = retained_directory_size(&run_root)?;
    if newest_run_bytes > retention_bytes {
        eprintln!(
            "warning: newest immutable run uses {newest_run_bytes} bytes, exceeding output.retention_bytes={retention_bytes}; retained newest run and pruned only older runs"
        );
    }
    let latest = analysis.output_root.join("latest");
    replace_latest_from_run(
        &latest,
        &run_root,
        yaml_enabled,
        compressed.as_ref().map(|(name, _)| name.as_str()),
    )?;
    enforce_retention(&runs_root, retention, retention_bytes)?;
    let compressed_report = compressed.map(|(name, _)| latest.join(name));

    Ok(FindResult {
        report,
        report_json: latest.join("report.json"),
        report_yaml: latest.join("report.yaml"),
        summary_md: latest.join("summary.md"),
        health_md: latest.join("health.md"),
        compressed_report,
        terminal,
        elapsed_ms: 0,
    })
}

#[cfg(test)]
mod profile_tests {
    use super::*;

    #[test]
    fn compact_profile_keeps_an_exhaustive_index_and_resolvable_references() {
        let files = (0..300)
            .map(|index| json!({"path": format!("src/{index:03}.rs")}))
            .collect::<Vec<_>>();
        let mut report = json!({
            "files": files.clone(),
            "folders": [],
            "compare_index": {"files": files, "folders": []},
            "ranked_files": [{"path": "src/297.rs"}],
            "action_queue": [{"path": "src/298.rs"}],
            "health": {
                "findings": [{"path": "src/299.rs"}],
                "refactor_candidates": [],
                "watchlist": []
            },
            "overlays": {"organization_health": {
                "relationships": {"temporal_coupling_edges": [{
                    "source_path": "src/298.rs", "target_path": "src/299.rs", "confidence": "supported"
                }]},
                "clusters": {"duplicate_sets": [{
                    "member_paths": ["src/298.rs", "src/299.rs"]
                }]}
            }},
            "summary": {},
            "diagnostics": {},
            "collection_metadata": {}
        });
        apply_report_profile(&mut report, "compact");
        let retained = report["files"]
            .as_array()
            .expect("compact files")
            .iter()
            .filter_map(|record| record["path"].as_str())
            .collect::<BTreeSet<_>>();
        assert_eq!(retained.len(), 250);
        for path in ["src/297.rs", "src/298.rs", "src/299.rs"] {
            assert!(retained.contains(path), "missing referenced path {path}");
        }
        assert_eq!(
            report["compare_index"]["files"].as_array().map(Vec::len),
            Some(300)
        );
        assert_eq!(
            report["health"]["findings"].as_array().map(Vec::len),
            Some(1)
        );
        assert_eq!(report["action_queue"].as_array().map(Vec::len), Some(1));
        assert_eq!(
            report["overlays"]["organization_health"]["relationships"]["temporal_coupling_edges"]
                .as_array()
                .map(Vec::len),
            Some(1)
        );
    }

    #[test]
    fn standard_bounds_high_cardinality_evidence_while_full_evidence_does_not() {
        let relationships = (0..2_100)
            .map(|index| json!({"id": index, "confidence": "supported"}))
            .collect::<Vec<_>>();
        let report = json!({
            "diagnostics": {},
            "overlays": {"organization_health": {"relationships": {
                "temporal_coupling_edges": relationships
            }}}
        });
        let mut standard = report.clone();
        apply_report_profile(&mut standard, "standard");
        let mut full = report;
        apply_report_profile(&mut full, "full_evidence");
        assert_eq!(
            standard
                .pointer("/overlays/organization_health/relationships/temporal_coupling_edges")
                .and_then(Value::as_array)
                .map(Vec::len),
            Some(500)
        );
        assert_eq!(
            full.pointer("/overlays/organization_health/relationships/temporal_coupling_edges")
                .and_then(Value::as_array)
                .map(Vec::len),
            Some(2_100)
        );
    }
}
