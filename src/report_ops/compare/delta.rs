fn require_compatible_reports(mismatches: &[Value]) -> Result<()> {
    if let Some(first) = mismatches
        .iter()
        .find(|mismatch| mismatch.get("blocking").and_then(Value::as_bool) != Some(false))
    {
        let label = string(first.get("field"));
        let base = first.get("base").cloned().unwrap_or(Value::Null);
        let head = first.get("head").cloned().unwrap_or(Value::Null);
        bail!(
            "reports have incompatible {label}: base={base}, head={head}; rerun compare with --force only if this mismatch is intentional"
        );
    }
    Ok(())
}

fn has_blocking_mismatches(mismatches: &[Value]) -> bool {
    mismatches
        .iter()
        .any(|mismatch| mismatch.get("blocking").and_then(Value::as_bool) != Some(false))
}

fn require_comparison_ready(
    report: &Value,
    role: &str,
    allow_incomplete_evidence: bool,
    force: bool,
) -> Result<()> {
    let readiness =
        evaluate_report_readiness(report, role == "base" && !force, allow_incomplete_evidence);
    if let Some(blocker) = readiness.blockers.first() {
        let code = string(blocker.get("code"));
        let pointer = string(blocker.get("pointer"));
        let message = string(blocker.get("message"));
        bail!(
            "{role} report is not comparison-ready ({code} at {pointer}): {message}; rerun `git slop find` with complete inputs"
        );
    }
    Ok(())
}

fn build_record_delta(path: &str, base: Option<&Value>, head: Option<&Value>) -> Value {
    let base_score = record_score(base);
    let head_score = record_score(head);
    let base_tokens = record_tokens(base);
    let head_tokens = record_tokens(head);
    let base_load = record_load_pressure(base);
    let head_load = record_load_pressure(head);
    let base_context = record_band(base, "context_band");
    let head_context = record_band(head, "context_band");
    let base_slop = record_band(base, "slop_band");
    let head_slop = record_band(head, "slop_band");
    let base_fingerprint = base.and_then(record_content_identity);
    let head_fingerprint = head.and_then(record_content_identity);
    let content_changed = match (&base_fingerprint, &head_fingerprint) {
        (Some(base), Some(head)) => Some(base != head),
        _ => None,
    };
    let metrics_changed = match (base, head) {
        (Some(base), Some(head)) => Some(metric_changed(base, head)),
        _ => None,
    };
    let content_status = match (base, head, content_changed) {
        (None, Some(_), _) => "added",
        (Some(_), None, _) => "removed",
        (Some(_), Some(_), Some(true)) => "changed",
        (Some(_), Some(_), Some(false)) => "unchanged",
        _ => "unknown",
    };
    let metric_status = match metrics_changed {
        Some(true) => "changed",
        Some(false) => "unchanged",
        None => "not_comparable",
    };
    let evidence_status = match (content_changed, metrics_changed) {
        (Some(false), Some(true)) => "drifted",
        (Some(_), Some(_)) => "stable",
        _ => "not_comparable",
    };
    let context_delta = band_delta(base_context.as_deref(), head_context.as_deref());
    let slop_delta = band_delta(base_slop.as_deref(), head_slop.as_deref());
    json!({
        "path": path,
        "status": record_delta_status(base, head),
        "content_status": content_status,
        "metric_status": metric_status,
        "evidence_status": evidence_status,
        "base_slop_score": base_score,
        "head_slop_score": head_score,
        "slop_score_delta": option_f64_delta(base_score, head_score),
        "base_tokens": base_tokens,
        "head_tokens": head_tokens,
        "token_delta": option_i64_delta(base_tokens, head_tokens),
        "base_load_pressure": base_load,
        "head_load_pressure": head_load,
        "load_pressure_delta": option_f64_delta(base_load, head_load),
        "base_context_band": base_context,
        "head_context_band": head_context,
        "context_band_delta": context_delta,
        "base_slop_band": base_slop,
        "head_slop_band": head_slop,
        "slop_band_delta": slop_delta,
        "base_content_fingerprint": base_fingerprint,
        "head_content_fingerprint": head_fingerprint,
        "content_changed": content_changed,
        "overlay_deltas": record_overlay_delta(base, head),
    })
}

fn regression_for_delta(delta: &Value, base: &Value) -> Option<Value> {
    let path = string(delta.get("path"));
    let status = string(delta.get("status"));
    let head_context_band = string(delta.get("head_context_band"));
    let head_slop_band = string(delta.get("head_slop_band"));
    let severity = if matches!(
        head_context_band.as_str(),
        "critical" | "refactor_required" | "budget_exceeded"
    ) || head_slop_band == "critical"
    {
        "error"
    } else if head_context_band == "warning" || head_slop_band == "high" {
        "warning"
    } else {
        "notice"
    };
    let reason = if status == "added" {
        if !matches!(
            head_context_band.as_str(),
            "warning" | "critical" | "refactor_required" | "budget_exceeded"
        ) && !matches!(head_slop_band.as_str(), "high" | "critical")
        {
            return None;
        }
        "new_finding"
    } else if matches!(status.as_str(), "source_changed" | "evidence_drift") {
        let context_delta = delta
            .get("context_band_delta")
            .and_then(Value::as_i64)
            .unwrap_or_default();
        let slop_delta = delta
            .get("slop_band_delta")
            .and_then(Value::as_i64)
            .unwrap_or_default();
        let score_delta = number(delta.get("slop_score_delta"));
        let threshold = base
            .pointer("/config/check/regression_score_delta")
            .and_then(Value::as_f64)
            .unwrap_or(5.0);
        let content_changed = delta
            .get("content_changed")
            .and_then(Value::as_bool)
            .unwrap_or(true);
        let fail_on_evidence_drift = base
            .pointer("/config/check/fail_on_evidence_drift")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        if status == "evidence_drift" && !fail_on_evidence_drift {
            return None;
        }
        if (context_delta > 0 || slop_delta > 0) && (content_changed || fail_on_evidence_drift) {
            "worse_band"
        } else if score_delta >= threshold && content_changed {
            "material_score_increase"
        } else {
            return None;
        }
    } else {
        return None;
    };
    Some(json!({
        "path": path,
        "status": if status == "added" { "new" } else { "worsened" },
        "reason": reason,
        "severity": severity,
        "base_slop_score": delta.get("base_slop_score").cloned().unwrap_or(Value::Null),
        "head_slop_score": delta.get("head_slop_score").cloned().unwrap_or(Value::Null),
        "slop_score_delta": delta.get("slop_score_delta").cloned().unwrap_or(Value::Null),
        "context_band": delta.get("head_context_band").cloned().unwrap_or(Value::Null),
        "slop_band": delta.get("head_slop_band").cloned().unwrap_or(Value::Null),
    }))
}

fn build_record_deltas(base: &Value, head: &Value, collection: &str) -> Vec<Value> {
    let base_records = records_by_path(base, collection);
    let head_records = records_by_path(head, collection);
    let fingerprint_paths = |records: &BTreeMap<String, Value>, other: &BTreeMap<String, Value>| {
        let mut values: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for (path, record) in records {
            if other.contains_key(path) {
                continue;
            }
            if let Some(fingerprint) = record_content_identity(record) {
                values.entry(fingerprint).or_default().push(path.clone());
            }
        }
        values
    };
    let base_fingerprints = fingerprint_paths(&base_records, &head_records);
    let head_fingerprints = fingerprint_paths(&head_records, &base_records);
    let mut renamed_base_paths = BTreeSet::new();
    let mut renamed_head_paths = BTreeSet::new();
    let mut renamed = Vec::new();
    for (fingerprint, old_paths) in &base_fingerprints {
        let Some(new_paths) = head_fingerprints.get(fingerprint) else {
            continue;
        };
        if old_paths.len() != 1 || new_paths.len() != 1 {
            continue;
        }
        let old_path = &old_paths[0];
        let new_path = &new_paths[0];
        renamed_base_paths.insert(old_path.clone());
        renamed_head_paths.insert(new_path.clone());
        let mut delta = build_record_delta(
            new_path,
            base_records.get(old_path),
            head_records.get(new_path),
        );
        delta["status"] = json!("renamed");
        delta["content_status"] = json!("renamed_unchanged");
        delta["renamed_from"] = json!(old_path);
        delta["renamed_to"] = json!(new_path);
        renamed.push(delta);
    }
    let paths: BTreeSet<String> = base_records
        .keys()
        .chain(head_records.keys())
        .filter(|path| !renamed_base_paths.contains(*path) && !renamed_head_paths.contains(*path))
        .cloned()
        .collect();
    let mut deltas = paths
        .into_iter()
        .map(|path| {
            let mut delta =
                build_record_delta(&path, base_records.get(&path), head_records.get(&path));
            if collection == "folders"
                && delta.get("status").and_then(Value::as_str) == Some("source_changed")
                && delta.get("content_status").and_then(Value::as_str) == Some("unknown")
            {
                delta["status"] = json!("aggregate_changed");
                delta["content_status"] = json!("not_applicable");
            }
            delta
        })
        .collect::<Vec<_>>();
    deltas.extend(renamed);
    deltas.sort_by_key(|left| string(left.get("path")));
    deltas
}

fn queue_positions(report: &Value) -> BTreeMap<String, usize> {
    let mut positions = BTreeMap::new();
    for (index, item) in array_at(report, &["action_queue"]).iter().enumerate() {
        if let Some(path) = item.get("path").and_then(Value::as_str) {
            positions.entry(path.to_string()).or_insert(index + 1);
        }
    }
    positions
}

fn build_queue_movement(base: &Value, head: &Value) -> Vec<Value> {
    let base_positions = queue_positions(base);
    let head_positions = queue_positions(head);
    let paths: BTreeSet<String> = base_positions
        .keys()
        .chain(head_positions.keys())
        .cloned()
        .collect();
    let mut result: Vec<Value> = paths
        .into_iter()
        .map(|path| {
            let base_position = base_positions.get(&path).copied();
            let head_position = head_positions.get(&path).copied();
            let (status, position_delta): (&str, Option<i64>) = match (base_position, head_position)
            {
                (None, Some(_)) => ("newly_queued", None),
                (Some(_), None) => ("dropped_from_queue", None),
                (Some(base), Some(head)) if head < base => {
                    ("moved_up", Some(head as i64 - base as i64))
                }
                (Some(base), Some(head)) if head > base => {
                    ("moved_down", Some(head as i64 - base as i64))
                }
                _ => ("unchanged_position", Some(0)),
            };
            json!({
                "path": path,
                "status": status,
                "base_position": base_position,
                "head_position": head_position,
                "position_delta": position_delta,
            })
        })
        .collect();
    result.sort_by(|left, right| {
        let rank = |status: &str| match status {
            "newly_queued" => 0,
            "moved_up" => 1,
            "moved_down" => 2,
            "dropped_from_queue" => 3,
            _ => 4,
        };
        rank(&string(left.get("status")))
            .cmp(&rank(&string(right.get("status"))))
            .then_with(|| {
                let left_position = left
                    .get("head_position")
                    .and_then(Value::as_u64)
                    .or_else(|| left.get("base_position").and_then(Value::as_u64))
                    .and_then(|value| usize::try_from(value).ok())
                    .unwrap_or(10_000);
                let right_position = right
                    .get("head_position")
                    .and_then(Value::as_u64)
                    .or_else(|| right.get("base_position").and_then(Value::as_u64))
                    .and_then(|value| usize::try_from(value).ok())
                    .unwrap_or(10_000);
                left_position.cmp(&right_position)
            })
            .then_with(|| string(left.get("path")).cmp(&string(right.get("path"))))
    });
    result
}

fn delta_counts(items: &[Value]) -> Value {
    let count = |status: &str| {
        items
            .iter()
            .filter(|item| string(item.get("status")) == status)
            .count()
    };
    json!({
        "added": count("added"),
        "removed": count("removed"),
        "changed": count("source_changed") + count("evidence_drift") + count("aggregate_changed"),
        "source_changed": count("source_changed"),
        "evidence_drift": count("evidence_drift"),
        "renamed": count("renamed"),
        "aggregate_changed": count("aggregate_changed"),
        "unchanged": count("unchanged"),
    })
}

fn aggregate_overlay_deltas(items: &[Value]) -> Vec<Value> {
    let mut aggregate: BTreeMap<String, f64> = BTreeMap::new();
    for item in items {
        for overlay in array_at(item, &["overlay_deltas"]) {
            let label = string(overlay.get("label"));
            let delta = number(overlay.get("delta"));
            aggregate
                .entry(label)
                .and_modify(|value| *value = round6(*value + delta))
                .or_insert(delta);
        }
    }
    let mut result: Vec<Value> = aggregate
        .into_iter()
        .filter(|(_, delta)| *delta != 0.0)
        .map(|(label, total_delta)| json!({"label": label, "total_delta": total_delta}))
        .collect();
    result.sort_by(|left, right| {
        cmp_f64_desc(
            number(left.get("total_delta")).abs(),
            number(right.get("total_delta")).abs(),
        )
        .then_with(|| string(left.get("label")).cmp(&string(right.get("label"))))
    });
    result
}

fn report_descriptor(report: &Value, path: Option<&str>) -> Value {
    let repo = report.get("repo").unwrap_or(&Value::Null);
    let report_digest = serde_json::to_vec(report)
        .map(|bytes| hex::encode(Sha256::digest(bytes)))
        .unwrap_or_default();
    json!({
        "path": path,
        "repo_name": repo.get("repo_name").cloned().unwrap_or(Value::Null),
        "head_sha": repo.get("head_sha").or_else(|| repo.get("head_commit")).cloned().unwrap_or(Value::Null),
        "generated_at": report.get("generated_at")
            .or_else(|| value_at(report, &["summary", "generated_at"]))
            .cloned()
            .unwrap_or(Value::Null),
        "analyzed_revision_at": report.get("analyzed_revision_at").cloned().unwrap_or(Value::Null),
        "schema_version": report.get("schema_version").cloned().unwrap_or(Value::Null),
        "report_digest": report_digest,
        "content_digest": repo.get("analyzed_content_digest").cloned().unwrap_or(Value::Null),
        "scope": report.get("scope").cloned().unwrap_or(Value::Null),
        "analyzer_version": report.pointer("/analyzer/version").cloned().unwrap_or(Value::Null),
        "analysis_contract_version": report.pointer("/analyzer/analysis_contract_version").cloned().unwrap_or(Value::Null),
        "config_digest": report.pointer("/analyzer/config_digest").cloned().unwrap_or(Value::Null),
        "analysis_config_digest": report.pointer("/analyzer/analysis_config_digest").cloned().unwrap_or(Value::Null),
        "context_tokenizer": report.pointer("/analyzer/context_tokenizer").cloned().unwrap_or(Value::Null),
        "history_complete": report.pointer("/stats/history_complete").cloned().unwrap_or(Value::Null),
    })
}
