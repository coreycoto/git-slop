fn run_check(repo_root: &Path, args: CheckArgs) -> Result<i32> {
    if args.details && !(1..=10_000).contains(&args.limit) {
        return Err(ClassifiedError::new(
            ErrorKind::Contract,
            "invalid_argument",
            "--limit must be between 1 and 10000",
        )
        .at("/limit")
        .with_details(json!({"flag": "--limit", "minimum": 1, "maximum": 10_000, "actual": args.limit}))
        .into());
    }
    let (loaded, _) = report_or_missing(repo_root, args.report.as_deref())?;
    let freshness = if args.require_current {
        Some(require_current_report(repo_root, &loaded)?)
    } else {
        None
    };
    let readiness = crate::report_ops::evaluate_report_readiness(
        &loaded,
        false,
        args.allow_incomplete_evidence,
    );
    if !readiness.comparison_ready {
        return Err(ClassifiedError::new(
            ErrorKind::Contract,
            "incomplete_evidence",
            "report is not enforcement-ready; rerun analysis with complete inputs or pass --allow-incomplete-evidence only for acknowledged evidence loss",
        )
        .at("/readiness")
        .with_details(readiness.as_json())
        .into());
    }
    let loaded_config = loaded
        .get("config")
        .cloned()
        .unwrap_or_else(config::default_config);
    let context_band = args
        .fail_on_context_band
        .map(ContextBand::as_str)
        .or_else(|| {
            loaded_config
                .pointer("/check/fail_on_context_band")
                .and_then(Value::as_str)
        })
        .unwrap_or("critical");
    let slop_band = args
        .fail_on_slop_band
        .map(SlopBand::as_str)
        .or_else(|| {
            loaded_config
                .pointer("/check/fail_on_slop_band")
                .and_then(Value::as_str)
        })
        .unwrap_or("critical");
    let failures = failing_records_in(
        &loaded,
        Some(context_band),
        Some(slop_band),
        args.include_folders,
    );
    if !matches!(args.format, CheckFormat::Text) {
        match args.format {
            CheckFormat::Json => {
                let mut payload = json!({
                    "schema_version": 1,
                    "command": "check",
                    "report": {"schema_version": loaded.get("schema_version"), "analyzer": loaded.get("analyzer"), "repo": loaded.get("repo"), "scope": loaded.get("scope")},
                    "boundary": {"context_band": context_band, "slop_band": slop_band},
                    "passed": failures.is_empty(),
                    "finding_count": failures.len(),
                    "details_included": args.details,
                    "gate_scope": if args.include_folders { "files_and_folders" } else { "files" },
                    "freshness": freshness,
                });
                if args.details {
                    let findings = failures
                        .iter()
                        .skip(args.offset)
                        .take(args.limit)
                        .cloned()
                        .collect::<Vec<_>>();
                    payload["findings"] = json!(findings);
                    payload["collection"] = json!({
                        "total": failures.len(),
                        "offset": args.offset,
                        "limit": args.limit,
                        "returned": payload["findings"].as_array().map(Vec::len).unwrap_or_default(),
                        "truncated": args.offset.saturating_add(args.limit) < failures.len(),
                    });
                }
                print_text(&render_json(&payload)?);
            }
            CheckFormat::Github => {
                for failure in &failures {
                    let path = crate::text::github_property_escape(
                        failure
                            .get("path")
                            .and_then(Value::as_str)
                            .unwrap_or_default(),
                    );
                    println!(
                        "::error file={}::Git Slop context={} slop={} score={}",
                        path,
                        failure
                            .get("context_band")
                            .and_then(Value::as_str)
                            .unwrap_or_default(),
                        failure
                            .get("slop_band")
                            .and_then(Value::as_str)
                            .unwrap_or_default(),
                        failure.get("slop_score").unwrap_or(&Value::Null)
                    );
                }
            }
            CheckFormat::Text => {}
        }
        return Ok(if failures.is_empty() || args.evaluate_only { 0 } else { 1 });
    }
    if failures.is_empty() {
        println!(
            "Check passed: no file records met or exceeded context={context_band} or slop={slop_band}."
        );
        return Ok(0);
    }
    println!(
        "Check failed: {} file records met or exceeded context={context_band} or slop={slop_band}.",
        failures.len()
    );
    for failure in failures.iter().take(10) {
        println!(
            "- {} (slop={}, context={}, slop_score={})",
            safe_terminal(
                failure
                    .get("path")
                    .and_then(Value::as_str)
                    .unwrap_or_default(),
            ),
            failure
                .get("slop_band")
                .and_then(Value::as_str)
                .unwrap_or_default(),
            failure
                .get("context_band")
                .and_then(Value::as_str)
                .unwrap_or_default(),
            failure
                .get("slop_score")
                .map(ToString::to_string)
                .unwrap_or_else(|| "null".to_string()),
        );
    }
    Ok(if args.evaluate_only { 0 } else { 1 })
}

fn bounded_compare_output(
    payload: &Value,
    detail: CompareDetail,
    top: usize,
    offset: usize,
    limit: usize,
    include_unchanged: bool,
) -> Result<Value> {
    if limit == 0 {
        anyhow::bail!("--limit must be greater than zero");
    }
    let mut bounded = payload.clone();
    let cap = match detail {
        CompareDetail::Summary => 0,
        CompareDetail::Top => top,
        CompareDetail::Full => limit,
    };
    let start = if matches!(detail, CompareDetail::Full) {
        offset
    } else {
        0
    };
    let mut pagination = serde_json::Map::new();
    for key in [
        "file_deltas",
        "folder_deltas",
        "queue_movement",
        "overlay_deltas",
        "regressions",
    ] {
        let all_values = payload
            .get(key)
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let changed_values = if key == "queue_movement" {
            all_values
                .iter()
                .filter(|item| {
                    item.get("status").and_then(Value::as_str) != Some("unchanged_position")
                })
                .cloned()
                .collect::<Vec<_>>()
        } else if matches!(key, "file_deltas" | "folder_deltas") {
            all_values
                .iter()
                .filter(|item| item.get("status").and_then(Value::as_str) != Some("unchanged"))
                .cloned()
                .collect::<Vec<_>>()
        } else {
            all_values.clone()
        };
        let values = if include_unchanged && matches!(key, "file_deltas" | "folder_deltas") {
            all_values.clone()
        } else {
            changed_values.clone()
        };
        let returned = values
            .iter()
            .skip(start)
            .take(cap)
            .cloned()
            .collect::<Vec<_>>();
        pagination.insert(
            key.to_string(),
            json!({
                "total": values.len(),
                "changed_total": changed_values.len(),
                "all_total": all_values.len(),
                "offset": start,
                "limit": cap,
                "returned": returned.len(),
                "has_more": start.saturating_add(returned.len()) < values.len()
            }),
        );
        bounded[key] = json!(returned);
    }
    bounded["detail"] = json!(match detail {
        CompareDetail::Summary => "summary",
        CompareDetail::Top => "top",
        CompareDetail::Full => "full",
    });
    bounded["pagination"] = Value::Object(pagination);
    bounded["include_unchanged"] = json!(include_unchanged);
    Ok(bounded)
}

fn render_compare_ndjson(payload: &Value) -> Result<String> {
    let mut lines = vec![serde_json::to_string(&json!({
        "record_type": "summary",
        "schema_version": payload.get("schema_version"),
        "stream": {
            "schema": "schemas/compare-ndjson-1.json",
            "record_types": ["summary", "file_delta", "folder_delta", "queue_movement", "overlay_delta", "regression"]
        },
        "summary": payload.get("summary"),
        "pagination": payload.get("pagination"),
        "baseline_status": payload.get("baseline_status")
    }))?];
    for (key, record_type) in [
        ("file_deltas", "file_delta"),
        ("folder_deltas", "folder_delta"),
        ("queue_movement", "queue_movement"),
        ("overlay_deltas", "overlay_delta"),
        ("regressions", "regression"),
    ] {
        for record in payload
            .get(key)
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            lines.push(serde_json::to_string(
                &json!({"record_type": record_type, "record": record}),
            )?);
        }
    }
    Ok(format!("{}\n", lines.join("\n")))
}

#[cfg(test)]
mod compare_pagination_tests {
    use super::*;

    #[test]
    fn unchanged_records_are_opt_in_and_do_not_create_phantom_pages() {
        let payload = json!({
            "file_deltas": [
                {"path":"same.rs","status":"unchanged"},
                {"path":"changed.rs","status":"changed"}
            ],
            "folder_deltas": [{"path":".","status":"unchanged"}],
            "queue_movement": [], "overlay_deltas": [], "regressions": []
        });
        let bounded = bounded_compare_output(&payload, CompareDetail::Top, 10, 0, 1000, false)
            .expect("bounded compare");
        assert_eq!(bounded["file_deltas"].as_array().map(Vec::len), Some(1));
        assert_eq!(bounded["folder_deltas"].as_array().map(Vec::len), Some(0));
        assert_eq!(bounded["pagination"]["file_deltas"]["all_total"], 2);
        assert_eq!(bounded["pagination"]["file_deltas"]["changed_total"], 1);
        assert_eq!(bounded["pagination"]["folder_deltas"]["has_more"], false);

        let complete = bounded_compare_output(&payload, CompareDetail::Top, 10, 0, 1000, true)
            .expect("complete compare");
        assert_eq!(complete["file_deltas"].as_array().map(Vec::len), Some(2));
        assert_eq!(complete["pagination"]["file_deltas"]["changed_total"], 1);
    }
}

#[cfg(test)]
mod ndjson_tests {
    use super::*;

    #[test]
    fn compare_ndjson_emits_one_complete_json_value_per_physical_line() {
        let payload = json!({
            "schema_version": 1,
            "summary": {},
            "pagination": {},
            "baseline_status": "compatible",
            "file_deltas": [{"path": "src/lib.rs", "nested": {"value": true}}],
            "folder_deltas": [],
            "queue_movement": [],
            "overlay_deltas": [],
            "regressions": []
        });
        let rendered = render_compare_ndjson(&payload).expect("ndjson");
        let lines = rendered.lines().collect::<Vec<_>>();
        assert_eq!(lines.len(), 2);
        for line in lines {
            assert!(!line.contains('\n'));
            serde_json::from_str::<Value>(line).expect("single-line JSON record");
        }
    }
}
