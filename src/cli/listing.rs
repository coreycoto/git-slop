struct ListQuery<'a> {
    output: &'a ListOutputArgs,
    path: Option<&'a str>,
    profile: Option<&'a str>,
    language: Option<&'a str>,
    classification: Option<&'a str>,
    severity: Option<&'a str>,
    context_band: Option<&'a str>,
    slop_band: Option<&'a str>,
}

fn findings_query(args: &FindingsListArgs) -> ListQuery<'_> {
    ListQuery {
        output: &args.output,
        path: args.path.as_deref(),
        profile: args.profile.as_deref(),
        language: args.language.as_deref(),
        classification: args.classification.as_deref(),
        severity: args.severity.as_deref(),
        context_band: args.context_band.as_deref(),
        slop_band: args.slop_band.as_deref(),
    }
}

fn list_query(command: &ListCommand) -> (&'static str, ListQuery<'_>) {
    match command {
        ListCommand::PolicyFailures(args) => ("policy_failures", findings_query(args)),
        ListCommand::Interventions(args) => ("interventions", findings_query(args)),
        ListCommand::Observations(args) => ("observations", findings_query(args)),
        ListCommand::HealthFindings(args) => ("health_findings", findings_query(args)),
        ListCommand::Findings(args) => ("findings", findings_query(args)),
        ListCommand::Relationships(args) => (
            "relationships",
            ListQuery {
                output: &args.output,
                path: args.path.as_deref(),
                profile: args.profile.as_deref(),
                language: args.language.as_deref(),
                classification: args.classification.as_deref(),
                severity: None,
                context_band: None,
                slop_band: None,
            },
        ),
        ListCommand::Clusters(args) => (
            "clusters",
            ListQuery {
                output: &args.output,
                path: args.path.as_deref(),
                profile: args.profile.as_deref(),
                language: args.language.as_deref(),
                classification: args.classification.as_deref(),
                severity: None,
                context_band: None,
                slop_band: None,
            },
        ),
        ListCommand::Profiles(args) => (
            "profiles",
            ListQuery {
                output: &args.output,
                path: None,
                profile: args.profile.as_deref(),
                language: None,
                classification: None,
                severity: None,
                context_band: None,
                slop_band: None,
            },
        ),
    }
}

fn matches_list_filter(
    item: &Value,
    query: &ListQuery<'_>,
    kind: &str,
    files: &std::collections::BTreeMap<String, Value>,
) -> bool {
    let candidate_paths = match kind {
        "relationships" => ["source_path", "target_path"]
            .into_iter()
            .filter_map(|key| item.get(key).and_then(Value::as_str))
            .collect::<Vec<_>>(),
        "clusters" => item
            .get("member_paths")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .collect(),
        _ => item
            .get("path")
            .and_then(Value::as_str)
            .into_iter()
            .collect(),
    };
    let field_matches = |field: &str, expected: &str| {
        item.get(field).and_then(Value::as_str) == Some(expected)
            || candidate_paths.iter().any(|path| {
                files
                    .get(*path)
                    .and_then(|file| file.get(field))
                    .and_then(Value::as_str)
                    == Some(expected)
            })
    };
    query
        .path
        .is_none_or(|path| candidate_paths.iter().any(|value| value.starts_with(path)))
        && query.profile.is_none_or(|value| {
            field_matches("profile", value)
                || (kind == "profiles" && item.get("name").and_then(Value::as_str) == Some(value))
        })
        && query
            .language
            .is_none_or(|value| field_matches("language", value))
        && query.classification.is_none_or(|value| {
            field_matches("classification", value) || field_matches("class", value)
        })
        && query
            .severity
            .is_none_or(|value| item.get("severity").and_then(Value::as_str) == Some(value))
        && query.context_band.is_none_or(|value| {
            item.get("context_band").and_then(Value::as_str) == Some(value)
        })
        && query.slop_band.is_none_or(|value| {
            item.get("slop_band").and_then(Value::as_str) == Some(value)
        })
}

fn terminal_field(value: &str, width: usize, no_truncate: bool) -> String {
    let value = safe_terminal(value).replace(['\n', '\t'], " ");
    if no_truncate || value.chars().count() <= width {
        return value;
    }
    if width <= 1 {
        return "…".to_string();
    }
    value.chars().take(width - 1).collect::<String>() + "…"
}

include!("listing/render.rs");

fn run_list(repo_root: &Path, args: ListArgs) -> Result<i32> {
    let (kind, query) = list_query(&args.command);
    if query.output.top == 0 {
        return Err(ClassifiedError::new(
            ErrorKind::Contract,
            "invalid_list_limit",
            "--top must be greater than zero.",
        )
        .at("/top")
        .with_details(json!({"flag": "--top", "actual": query.output.top}))
        .into());
    }
    let (loaded, _) = report_or_missing_with_currentness(
        repo_root,
        query.output.report.as_deref(),
        query.output.require_current,
    )?;
    let files = loaded
        .get("files")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|record| Some((record.get("path")?.as_str()?.to_string(), record.clone())))
        .collect::<std::collections::BTreeMap<_, _>>();
    let mut values = match &args.command {
        ListCommand::PolicyFailures(_) => failing_records_in(
            &loaded,
            loaded
                .pointer("/policy_evaluation/thresholds/context_band")
                .and_then(Value::as_str),
            loaded
                .pointer("/policy_evaluation/thresholds/slop_band")
                .and_then(Value::as_str),
            false,
        ),
        ListCommand::Interventions(_) => loaded
            .pointer("/action_queue")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default(),
        ListCommand::Observations(_) => loaded
            .pointer("/observation_feed")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default(),
        ListCommand::HealthFindings(_) | ListCommand::Findings(_) => loaded
            .pointer("/health/findings")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default(),
        ListCommand::Relationships(_) => loaded
            .pointer("/overlays/organization_health/relationships")
            .and_then(Value::as_object)
            .into_iter()
            .flat_map(|map| map.values())
            .filter_map(Value::as_array)
            .flatten()
            .cloned()
            .collect(),
        ListCommand::Clusters(_) => deduplicate_clusters(
            loaded
                .pointer("/overlays/organization_health/clusters")
                .and_then(Value::as_object)
                .into_iter()
                .flat_map(|map| map.values())
                .filter_map(Value::as_array)
                .flatten()
                .cloned()
                .collect(),
        ),
        ListCommand::Profiles(_) => loaded
            .pointer("/health/profile_rollups")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default(),
    };
    let unfiltered_total = values.len();
    values.retain(|item| matches_list_filter(item, &query, kind, &files));
    rank_list_values(kind, &mut values);
    let matched_total = values.len();
    values.truncate(query.output.top);
    let returned = values.len();
    match query.output.format {
        DisplayFormat::Json => print_text(&render_json(&json!({
            "schema_version": 1,
            "command": "list",
            "kind": kind,
            "items": values,
            "collection": {"total": unfiltered_total, "matched": matched_total, "returned": returned, "limit": query.output.top, "truncated": returned < matched_total}
        }))?),
        DisplayFormat::Yaml => print_text(&serde_yaml::to_string(&values)?),
        DisplayFormat::Text => {
            match kind {
                "policy_failures" | "interventions" | "observations" | "health_findings" | "findings" => render_findings_table(&values, query.output),
                "relationships" => render_relationships_table(&values, query.output),
                "clusters" => render_clusters_table(&values, query.output),
                "profiles" => render_profiles_table(&values),
                _ => unreachable!(),
            }
            println!(
                "\nReturned {returned} of {matched_total} matching record(s) from {unfiltered_total} total.{}",
                if returned < matched_total {
                    " Increase --top to see more."
                } else {
                    ""
                }
            );
        }
    }
    Ok(0)
}

struct RunPrunePlan {
    before_runs: usize,
    before_bytes: u64,
    selected: Vec<(PathBuf, u64)>,
    retained_runs: usize,
    retained_bytes: u64,
}

fn plan_run_prune(root: &Path, keep: usize, max_bytes: u64) -> Result<RunPrunePlan> {
    let mut runs = if root.exists() {
        fs::read_dir(root)?
            .filter_map(Result::ok)
            .filter(|entry| {
                entry.file_type().is_ok_and(|kind| kind.is_dir())
                    && entry
                        .file_name()
                        .to_str()
                        .is_some_and(|name| !name.starts_with('.'))
            })
            .map(|entry| {
                let path = entry.path();
                let bytes = directory_size(&path)?;
                Ok((path, bytes))
            })
            .collect::<Result<Vec<_>>>()?
    } else {
        Vec::new()
    };
    runs.sort_by_key(|(path, _)| {
        std::cmp::Reverse(path.file_name().map(ToOwned::to_owned).unwrap_or_default())
    });
    let before_bytes = runs.iter().map(|(_, bytes)| *bytes).sum::<u64>();
    let before_runs = runs.len();
    let mut retained_bytes = 0_u64;
    let mut retained_runs = 0_usize;
    let mut selected = Vec::new();
    let mut retention_prefix_exhausted = false;
    for (index, (path, bytes)) in runs.into_iter().enumerate() {
        let retain_newest_even_if_oversized = index == 0 && keep > 0;
        if !retention_prefix_exhausted
            && (retain_newest_even_if_oversized
                || (retained_runs < keep
                    && retained_bytes.saturating_add(bytes) <= max_bytes))
        {
            retained_runs += 1;
            retained_bytes = retained_bytes.saturating_add(bytes);
        } else {
            retention_prefix_exhausted = true;
            selected.push((path, bytes));
        }
    }
    Ok(RunPrunePlan {
        before_runs,
        before_bytes,
        selected,
        retained_runs,
        retained_bytes,
    })
}

fn prune_plan_payload(plan: &RunPrunePlan, dry_run: bool) -> Value {
    let removed_bytes = plan.selected.iter().map(|(_, bytes)| *bytes).sum::<u64>();
    json!({
        "before": {"runs": plan.before_runs, "bytes": plan.before_bytes},
        "selected": plan.selected.iter().map(|(path, bytes)| json!({"path": path, "bytes": bytes})).collect::<Vec<_>>(),
        "removed": {"runs": plan.selected.len(), "bytes": removed_bytes},
        "after": {"runs": plan.retained_runs, "bytes": plan.retained_bytes, "projected": dry_run}
    })
}

fn apply_run_prune(plan: &RunPrunePlan, dry_run: bool) -> Result<()> {
    if !dry_run {
        for (path, _) in &plan.selected {
            fs::remove_dir_all(path)?;
        }
    }
    Ok(())
}

fn render_prune_plan(label: &str, plan: &RunPrunePlan, dry_run: bool) {
    for (path, _) in &plan.selected {
        println!(
            "{} {label} {}",
            if dry_run { "Would remove" } else { "Removing" },
            path.display()
        );
    }
    let removed_bytes = plan.selected.iter().map(|(_, bytes)| *bytes).sum::<u64>();
    println!(
        "{} {} old {label} snapshot(s) ({} bytes); retained {} ({} bytes).",
        if dry_run { "Selected" } else { "Pruned" },
        plan.selected.len(),
        removed_bytes,
        plan.retained_runs,
        plan.retained_bytes
    );
}

fn run_prune(repo_root: &Path, args: PruneArgs) -> Result<i32> {
    let dry_run = args.dry_run || !args.yes;
    let loaded = config::load(repo_root).unwrap_or_else(|_| config::default_config());
    let keep = args
        .keep
        .unwrap_or_else(|| config::pointer_u64(&loaded, "/output/retention_runs", 20) as usize);
    let max_bytes = args
        .max_bytes
        .unwrap_or_else(|| config::pointer_u64(&loaded, "/output/retention_bytes", 2_147_483_648));
    let state_root = match args.state_dir {
        Some(path) => resolve_repo_path(repo_root, &path),
        None => config::active_state_dir(repo_root)?,
    };
    let detector = plan_run_prune(&state_root.join("runs"), keep, max_bytes)?;
    let advice = plan_run_prune(&state_root.join("advice/runs"), keep, max_bytes)?;
    apply_run_prune(&detector, dry_run)?;
    apply_run_prune(&advice, dry_run)?;
    let detector_payload = prune_plan_payload(&detector, dry_run);
    let advice_payload = prune_plan_payload(&advice, dry_run);
    let payload = json!({
        "schema_version": 1,
        "command": "prune",
        "dry_run": dry_run,
        "apply_flag": "--yes",
        "limits": {"max_runs": keep, "max_bytes": max_bytes},
        "before": detector_payload["before"],
        "selected": detector_payload["selected"],
        "removed": detector_payload["removed"],
        "after": detector_payload["after"],
        "advice": advice_payload
    });
    match args.format {
        DisplayFormat::Text => {
            render_prune_plan("detector run", &detector, dry_run);
            render_prune_plan("advice run", &advice, dry_run);
            if dry_run && (!detector.selected.is_empty() || !advice.selected.is_empty()) {
                println!("Preview only; re-run with --yes to apply these removals.");
            }
        }
        DisplayFormat::Json => print_text(&render_json(&payload)?),
        DisplayFormat::Yaml => print_text(&serde_yaml::to_string(&payload)?),
    }
    Ok(0)
}

fn directory_size(path: &Path) -> Result<u64> {
    let mut total = 0u64;
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let metadata = fs::symlink_metadata(entry.path())?;
        if metadata.is_dir() {
            total = total.saturating_add(directory_size(&entry.path())?);
        } else {
            total = total.saturating_add(metadata.len());
        }
    }
    Ok(total)
}

fn run_cache(repo_root: &Path, args: CacheArgs) -> Result<i32> {
    let state_root = match args.state_dir {
        Some(path) => resolve_repo_path(repo_root, &path),
        None => config::active_state_dir(repo_root)?,
    };
    let (payload, format) = match args.command {
        CacheCommand::Status { format } => (crate::cache::status(&state_root)?, format),
        CacheCommand::Prune {
            max_entries,
            max_bytes,
            dry_run,
            yes,
            compact,
            format,
        } => (
            crate::cache::prune(
                &state_root,
                max_entries,
                max_bytes,
                dry_run || !yes,
                compact,
            )?,
            format,
        ),
    };
    match format {
        DisplayFormat::Json => print_text(&render_json(&payload)?),
        DisplayFormat::Yaml => print_text(&serde_yaml::to_string(&payload)?),
        DisplayFormat::Text => {
            if payload["command"] == "cache status" {
                println!(
                    "Cache {}: {} entries, {} payload bytes, {} database bytes.",
                    payload["status"].as_str().unwrap_or("unknown"),
                    payload["entries"],
                    payload["payload_bytes"],
                    payload["database_bytes"]
                );
            } else {
                let dry_run = payload["dry_run"].as_bool().unwrap_or(false);
                println!(
                    "{} {} cache entries ({} payload bytes).",
                    if dry_run { "Would prune" } else { "Pruned" },
                    payload["removed_entries"],
                    payload["removed_payload_bytes"]
                );
                if dry_run && payload["removed_entries"].as_u64().unwrap_or_default() > 0 {
                    println!("Preview only; re-run with --yes to apply these removals.");
                }
            }
        }
    }
    Ok(0)
}
