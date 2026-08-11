use super::*;
use crate::text::visible_controls;

fn optional_number(value: Option<&Value>) -> Option<f64> {
    value.and_then(Value::as_f64).map(round6)
}

fn optional_integer(value: Option<&Value>) -> Option<i64> {
    value.and_then(Value::as_i64)
}

fn records_by_path(report: &Value, collection: &str) -> BTreeMap<String, Value> {
    let records = report
        .pointer(&format!("/compare_index/{collection}"))
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_else(|| array_at(report, &[collection]));
    records
        .iter()
        .filter_map(|record| {
            record
                .get("path")
                .and_then(Value::as_str)
                .map(|path| (path.to_string(), record.clone()))
        })
        .collect()
}

fn record_score(record: Option<&Value>) -> Option<f64> {
    optional_number(record.and_then(|record| record.get("slop_score")))
}

fn record_load_pressure(record: Option<&Value>) -> Option<f64> {
    optional_number(record.and_then(|record| value_at(record, &["costs", "load", "load_pressure"])))
}

fn record_tokens(record: Option<&Value>) -> Option<i64> {
    record.and_then(|record| {
        optional_integer(record.get("tokens"))
            .or_else(|| optional_integer(value_at(record, &["costs", "load", "file_token_count"])))
    })
}

fn record_band(record: Option<&Value>, key: &str) -> Option<String> {
    optional_string(record.and_then(|record| record.get(key)))
}

fn band_rank(value: &str) -> i64 {
    match value {
        "compact" | "low" => 0,
        "healthy" | "moderate" => 1,
        "warning" | "high" => 2,
        "critical" | "refactor_required" | "budget_exceeded" => 3,
        _ => 0,
    }
}

fn band_delta(base: Option<&str>, head: Option<&str>) -> Option<i64> {
    Some(band_rank(head?) - band_rank(base?))
}

fn overlay_pressures(record: Option<&Value>) -> BTreeMap<String, f64> {
    strongest_pressures(record.and_then(|record| record.get("overlays")), 20)
        .into_iter()
        .map(|(label, value)| (label, round6(value)))
        .collect()
}

fn record_overlay_delta(base: Option<&Value>, head: Option<&Value>) -> Vec<Value> {
    let base_values = overlay_pressures(base);
    let head_values = overlay_pressures(head);
    let labels: BTreeSet<String> = base_values
        .keys()
        .chain(head_values.keys())
        .cloned()
        .collect();
    let mut changes: Vec<Value> = labels
        .into_iter()
        .filter_map(|label| {
            let base_value = base_values.get(&label).copied().unwrap_or(0.0);
            let head_value = head_values.get(&label).copied().unwrap_or(0.0);
            let delta = round6(head_value - base_value);
            (delta != 0.0).then(|| {
                json!({
                    "label": label,
                    "base": base_value,
                    "head": head_value,
                    "delta": delta,
                })
            })
        })
        .collect();
    changes.sort_by(|left, right| {
        cmp_f64_desc(
            number(left.get("delta")).abs(),
            number(right.get("delta")).abs(),
        )
        .then_with(|| string(left.get("label")).cmp(&string(right.get("label"))))
    });
    changes.truncate(5);
    changes
}

fn metric_changed(base: &Value, head: &Value) -> bool {
    record_score(Some(base)) != record_score(Some(head))
        || record_tokens(Some(base)) != record_tokens(Some(head))
        || record_load_pressure(Some(base)) != record_load_pressure(Some(head))
        || record_band(Some(base), "context_band") != record_band(Some(head), "context_band")
        || record_band(Some(base), "slop_band") != record_band(Some(head), "slop_band")
        || !record_overlay_delta(Some(base), Some(head)).is_empty()
}

fn record_content_identity(record: &Value) -> Option<String> {
    optional_string(record.get("content_sha256"))
        .filter(|value| !value.is_empty())
        .or_else(|| {
            optional_string(record.get("content_fingerprint"))
                .filter(|value| !value.is_empty() && !value.starts_with("incomplete:"))
        })
}

fn record_delta_status(base: Option<&Value>, head: Option<&Value>) -> &'static str {
    match (base, head) {
        (None, Some(_)) => "added",
        (Some(_), None) => "removed",
        (None, None) => "unchanged",
        (Some(base), Some(head))
            if record_content_identity(base).is_some()
                && record_content_identity(head).is_some()
                && record_content_identity(base) != record_content_identity(head) =>
        {
            "source_changed"
        }
        (Some(base), Some(head)) if !metric_changed(base, head) => "unchanged",
        (Some(base), Some(head))
            if record_content_identity(base).is_some()
                && record_content_identity(head).is_some()
                && record_content_identity(base) == record_content_identity(head) =>
        {
            "evidence_drift"
        }
        _ => "source_changed",
    }
}

fn option_f64_delta(base: Option<f64>, head: Option<f64>) -> Option<f64> {
    Some(round6(head? - base?))
}

fn option_i64_delta(base: Option<i64>, head: Option<i64>) -> Option<i64> {
    Some(head? - base?)
}

fn compatibility_value<'a>(report: &'a Value, pointer: &str) -> Option<&'a Value> {
    report.pointer(pointer)
}

fn repository_identity(report: &Value) -> Option<&Value> {
    report
        .pointer("/repo/repository_id")
        .or_else(|| report.pointer("/repo/remote_url"))
}

fn compatibility_mismatches(base: &Value, head: &Value) -> Vec<Value> {
    let mut mismatches = Vec::new();
    let base_identity = repository_identity(base);
    let head_identity = repository_identity(head);
    if base_identity.is_none() || head_identity.is_none() || base_identity != head_identity {
        mismatches.push(json!({
            "field": "repository identity",
            "pointer": "/repo/repository_id",
            "base": base_identity.cloned().unwrap_or(Value::Null),
            "head": head_identity.cloned().unwrap_or(Value::Null),
            "code": if base_identity.is_none() || head_identity.is_none() { "repository_identity_unavailable" } else { "repository_identity_mismatch" }
        }));
    }
    let base_profile = base.pointer("/analyzer/report_profile");
    let head_profile = head.pointer("/analyzer/report_profile");
    if base_profile != head_profile {
        mismatches.push(json!({
            "field": "report profile",
            "pointer": "/analyzer/report_profile",
            "base": base_profile.cloned().unwrap_or(Value::Null),
            "head": head_profile.cloned().unwrap_or(Value::Null),
            "code": "presentation_profile_mismatch",
            "blocking": false
        }));
    }
    for (label, pointer, fallback) in [
        (
            "tokenizer",
            "/analyzer/context_tokenizer",
            "/analyzer/context_tokenizer",
        ),
        (
            "analysis configuration digest",
            "/analyzer/analysis_config_digest",
            "/analyzer/config_digest",
        ),
        (
            "analysis contract",
            "/analyzer/analysis_contract_version",
            "/analyzer/version",
        ),
        (
            "evidence configuration digest",
            "/analyzer/evidence_config_digest",
            "/analyzer/config_digest",
        ),
        ("analysis scope mode", "/scope/mode", "/scope/mode"),
        ("analysis scope path", "/scope/path", "/scope/path"),
    ] {
        let left =
            compatibility_value(base, pointer).or_else(|| compatibility_value(base, fallback));
        let right =
            compatibility_value(head, pointer).or_else(|| compatibility_value(head, fallback));
        if left != right {
            mismatches.push(json!({
                "field": label,
                "pointer": pointer,
                "base": left.cloned().unwrap_or(Value::Null),
                "head": right.cloned().unwrap_or(Value::Null),
            }));
        }
    }
    mismatches
}

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

#[cfg(test)]
fn compare_payload_with_options(
    base_report: &Value,
    head_report: &Value,
    base_path: Option<&str>,
    head_path: Option<&str>,
    top: usize,
    force: bool,
    allow_incomplete_evidence: bool,
) -> Result<Value> {
    compare_payload_with_policy(
        base_report,
        head_report,
        base_path,
        head_path,
        top,
        force,
        allow_incomplete_evidence,
        "base",
    )
}

#[allow(clippy::too_many_arguments)]
pub fn compare_payload_with_policy(
    base_report: &Value,
    head_report: &Value,
    base_path: Option<&str>,
    head_path: Option<&str>,
    top: usize,
    force: bool,
    allow_incomplete_evidence: bool,
    policy_source: &str,
) -> Result<Value> {
    if !matches!(policy_source, "base" | "head") {
        bail!("comparison policy source must be base or head");
    }
    if report_schema(base_report) != REPORT_SCHEMA_VERSION {
        bail!("base report must use schema {REPORT_SCHEMA_VERSION}.");
    }
    if report_schema(head_report) != REPORT_SCHEMA_VERSION {
        bail!("head report must use schema {REPORT_SCHEMA_VERSION}.");
    }
    if top == 0 {
        bail!("--top must be greater than zero.");
    }
    require_comparison_ready(base_report, "base", allow_incomplete_evidence, force)?;
    require_comparison_ready(head_report, "head", allow_incomplete_evidence, force)?;
    if let (Some(base_time), Some(head_time)) = (
        base_report
            .get("generated_at")
            .and_then(Value::as_str)
            .and_then(|value| chrono::DateTime::parse_from_rfc3339(value).ok()),
        head_report
            .get("generated_at")
            .and_then(Value::as_str)
            .and_then(|value| chrono::DateTime::parse_from_rfc3339(value).ok()),
    ) {
        if base_time > head_time + chrono::Duration::hours(1) {
            bail!("base report timestamp is implausibly later than the head report timestamp");
        }
    }
    let compatibility_mismatches = compatibility_mismatches(base_report, head_report);
    let blocking_mismatches = has_blocking_mismatches(&compatibility_mismatches);
    if !force {
        require_compatible_reports(&compatibility_mismatches)?;
    }
    let file_deltas = build_record_deltas(base_report, head_report, "files");
    let folder_deltas = build_record_deltas(base_report, head_report, "folders");
    let worsened = file_deltas
        .iter()
        .filter(|item| {
            matches!(
                string(item.get("status")).as_str(),
                "source_changed" | "evidence_drift"
            ) && number(item.get("slop_score_delta")) > 0.0
        })
        .count();
    let improved = file_deltas
        .iter()
        .filter(|item| {
            matches!(
                string(item.get("status")).as_str(),
                "source_changed" | "evidence_drift"
            ) && number(item.get("slop_score_delta")) < 0.0
        })
        .count();
    let source_worsened = file_deltas
        .iter()
        .filter(|item| {
            matches!(
                string(item.get("status")).as_str(),
                "source_changed" | "added"
            ) && number(item.get("slop_score_delta")) > 0.0
        })
        .count();
    let evidence_only_worsened = file_deltas
        .iter()
        .filter(|item| {
            string(item.get("status")) == "evidence_drift"
                && number(item.get("slop_score_delta")) > 0.0
        })
        .count();
    let queue_movement = build_queue_movement(base_report, head_report);
    let overlay_deltas = aggregate_overlay_deltas(&file_deltas);
    let policy_report = if policy_source == "head" {
        head_report
    } else {
        base_report
    };
    let regressions = file_deltas
        .iter()
        .filter_map(|delta| regression_for_delta(delta, policy_report))
        .collect::<Vec<_>>();
    Ok(json!({
        "schema_version": COMPARE_SCHEMA_VERSION,
        "report_schema_version": REPORT_SCHEMA_VERSION,
        "command": "compare",
        "policy_source": policy_source,
        "base_report": report_descriptor(base_report, base_path),
        "head_report": report_descriptor(head_report, head_path),
        "summary": {
            "files": delta_counts(&file_deltas),
            "folders": delta_counts(&folder_deltas),
            "worsened_file_count": worsened,
            "source_worsened_file_count": source_worsened,
            "evidence_only_worsened_file_count": evidence_only_worsened,
            "improved_file_count": improved,
            "regression_count": regressions.len(),
        },
        "file_deltas": file_deltas,
        "folder_deltas": folder_deltas,
        "queue_movement": queue_movement,
        "overlay_deltas": overlay_deltas,
        "regressions": regressions,
        "boundary_note": COMPARE_BOUNDARY_NOTE,
        "compatibility_forced": force,
        "baseline_status": if !blocking_mismatches { "compatible" } else if force { "forced_incompatible" } else { "incompatible" },
        "baseline_compatible": !blocking_mismatches,
        "compatibility_mismatches": compatibility_mismatches,
    }))
}

pub fn render_compare_text(payload: &Value, top: usize) -> String {
    let base_path = visible_controls(&string(value_at(payload, &["base_report", "path"])));
    let head_path = visible_controls(&string(value_at(payload, &["head_report", "path"])));
    let basename = |value: &str, fallback: &str| {
        std::path::Path::new(value)
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or(fallback)
            .to_string()
    };
    let base_short = basename(&base_path, "<base>");
    let head_short = basename(&head_path, "<head>");
    let (base_name, head_name) = if base_short == head_short && !base_path.is_empty() {
        (base_path, head_path)
    } else {
        (base_short, head_short)
    };
    let base_digest = string(value_at(payload, &["base_report", "report_digest"]));
    let head_digest = string(value_at(payload, &["head_report", "report_digest"]));
    if !base_digest.is_empty() && base_digest == head_digest {
        return format!(
            "Compare: {base_name} -> {head_name}\n\nIdentical reports (sha256:{base_digest}).\n\n{}",
            string(payload.get("boundary_note"))
        );
    }
    let mut lines = vec![
        format!("Compare: {base_name} -> {head_name}"),
        String::new(),
        "Summary".to_string(),
        format!(
            "- files: added={}, removed={}, changed={}, unchanged={}",
            integer(value_at(payload, &["summary", "files", "added"])),
            integer(value_at(payload, &["summary", "files", "removed"])),
            integer(value_at(payload, &["summary", "files", "changed"])),
            integer(value_at(payload, &["summary", "files", "unchanged"])),
        ),
        format!(
            "- folders: added={}, removed={}, changed={}, unchanged={}",
            integer(value_at(payload, &["summary", "folders", "added"])),
            integer(value_at(payload, &["summary", "folders", "removed"])),
            integer(value_at(payload, &["summary", "folders", "changed"])),
            integer(value_at(payload, &["summary", "folders", "unchanged"])),
        ),
        format!(
            "- slop score movement: worsened_files={}, improved_files={}",
            integer(value_at(payload, &["summary", "worsened_file_count"])),
            integer(value_at(payload, &["summary", "improved_file_count"])),
        ),
        format!(
            "- policy regressions: {} (source-worsened={} can be higher because regression thresholds and new-finding policy are applied separately)",
            integer(value_at(payload, &["summary", "regression_count"])),
            integer(value_at(
                payload,
                &["summary", "source_worsened_file_count"]
            )),
        ),
        String::new(),
        "Top Worsened Files".to_string(),
    ];
    let mut worsened: Vec<&Value> = array_at(payload, &["file_deltas"])
        .iter()
        .filter(|item| number(item.get("slop_score_delta")) > 0.0)
        .collect();
    worsened.sort_by(|left, right| {
        cmp_f64_desc(
            number(left.get("slop_score_delta")),
            number(right.get("slop_score_delta")),
        )
        .then_with(|| string(left.get("path")).cmp(&string(right.get("path"))))
    });
    if worsened.is_empty() {
        lines.push("- none".to_string());
    } else {
        lines.extend(worsened.into_iter().take(top).map(|item| {
            format!(
                "- {}: {} -> {} (delta={})",
                visible_controls(&string(item.get("path"))),
                json_scalar_text(item.get("base_slop_score")),
                json_scalar_text(item.get("head_slop_score")),
                json_scalar_text(item.get("slop_score_delta")),
            )
        }));
    }
    lines.extend([String::new(), "Top Improved Files".to_string()]);
    let mut improved: Vec<&Value> = array_at(payload, &["file_deltas"])
        .iter()
        .filter(|item| number(item.get("slop_score_delta")) < 0.0)
        .collect();
    improved.sort_by(|left, right| {
        number(left.get("slop_score_delta"))
            .partial_cmp(&number(right.get("slop_score_delta")))
            .unwrap_or(Ordering::Equal)
            .then_with(|| string(left.get("path")).cmp(&string(right.get("path"))))
    });
    if improved.is_empty() {
        lines.push("- none".to_string());
    } else {
        lines.extend(improved.into_iter().take(top).map(|item| {
            format!(
                "- {}: {} -> {} (delta={})",
                visible_controls(&string(item.get("path"))),
                json_scalar_text(item.get("base_slop_score")),
                json_scalar_text(item.get("head_slop_score")),
                json_scalar_text(item.get("slop_score_delta")),
            )
        }));
    }
    lines.extend([String::new(), "Queue Movement".to_string()]);
    let movement = array_at(payload, &["queue_movement"]);
    if movement.is_empty() {
        lines.push("- none".to_string());
    } else {
        lines.extend(movement.iter().take(top).map(|item| {
            format!(
                "- {}: {} base={} head={}",
                visible_controls(&string(item.get("path"))),
                string(item.get("status")),
                json_scalar_text(item.get("base_position")),
                json_scalar_text(item.get("head_position")),
            )
        }));
    }
    lines.extend([String::new(), string(payload.get("boundary_note"))]);
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(path: &str, score: f64) -> Value {
        json!({
            "path": path,
            "content_fingerprint": format!("fingerprint-{path}"),
            "analysis_status": "analyzed",
            "tokens": 10,
            "context_band": "compact",
            "slop_score": score,
            "slop_band": if score >= 50.0 { "high" } else { "low" },
            "costs": {"load": {"load_pressure": score / 100.0}},
            "overlays": {}
        })
    }

    fn report(profile: &str, records: Vec<Value>) -> Value {
        let returned = records.len();
        let policy_records = records
            .iter()
            .map(|record| {
                json!({
                    "path": record.get("path"),
                    "classification": "source",
                    "profile": "agent_context",
                    "generated_from": [],
                    "tokens": record.get("tokens"),
                    "context_band": record.get("context_band"),
                    "slop_score": record.get("slop_score"),
                    "slop_band": record.get("slop_band"),
                    "reason_codes": []
                })
            })
            .collect::<Vec<_>>();
        json!({
            "schema_version": 5,
            "analyzer": {
                "report_profile": profile,
                "context_tokenizer": "cl100k_base",
                "analysis_config_digest": "analysis",
                "evidence_config_digest": "evidence",
                "analysis_contract_version": 2
            },
            "repo": {"repository_id": "repo"},
            "scope": {"mode": "repository", "path": null},
            "files": records.iter().take(250).cloned().collect::<Vec<_>>(),
            "folders": [],
            "compare_index": {"files": records, "folders": []},
            "policy_index": {"files": policy_records, "folders": []},
            "action_queue": [],
            "collection_metadata": {
                "compare_index": {
                    "files": {"total": returned, "returned": returned, "limit": null, "truncated": false},
                    "folders": {"total": 0, "returned": 0, "limit": null, "truncated": false}
                },
                "policy_index": {
                    "files": {"total": returned, "returned": returned, "limit": null, "truncated": false},
                    "folders": {"total": 0, "returned": 0, "limit": null, "truncated": false}
                }
            },
            "evidence_completeness": {"history": "complete"},
            "diagnostics": {"analysis": {"analysis_status": "complete"}}
        })
    }

    #[test]
    fn unchanged_compact_and_full_reports_compare_via_exhaustive_index() {
        let records = (0..300)
            .map(|index| record(&format!("src/{index:03}.rs"), index as f64 / 10.0))
            .collect::<Vec<_>>();
        let payload = compare_payload_with_options(
            &report("compact", records.clone()),
            &report("full_evidence", records),
            None,
            None,
            10,
            false,
            false,
        )
        .expect("cross-profile comparison");
        assert_eq!(payload["summary"]["files"]["added"], 0);
        assert_eq!(payload["summary"]["files"]["removed"], 0);
        assert_eq!(payload["summary"]["files"]["changed"], 0);
        assert_eq!(payload["summary"]["files"]["unchanged"], 300);
        assert_eq!(payload["baseline_compatible"], true);
        assert_eq!(
            payload["compatibility_mismatches"][0]["code"],
            "presentation_profile_mismatch"
        );
    }

    #[test]
    fn compact_rank_shift_does_not_create_phantom_additions_or_removals() {
        let base = (0..300)
            .map(|index| record(&format!("src/{index:03}.rs"), index as f64 / 10.0))
            .collect::<Vec<_>>();
        let mut head = base.clone();
        head[299] = record("src/299.rs", 99.0);
        let payload = compare_payload_with_options(
            &report("compact", base),
            &report("compact", head),
            None,
            None,
            10,
            false,
            false,
        )
        .expect("compact comparison");
        assert_eq!(payload["summary"]["files"]["added"], 0);
        assert_eq!(payload["summary"]["files"]["removed"], 0);
        assert_eq!(payload["summary"]["files"]["changed"], 1);
        assert_eq!(payload["summary"]["files"]["unchanged"], 299);
    }

    #[test]
    fn unique_content_fingerprint_is_reported_as_a_rename() {
        let mut old = record("src/old.rs", 20.0);
        let mut new = record("src/new.rs", 20.0);
        old["content_fingerprint"] = json!("same-content");
        new["content_fingerprint"] = json!("same-content");
        let payload = compare_payload_with_options(
            &report("standard", vec![old]),
            &report("standard", vec![new]),
            None,
            None,
            10,
            false,
            false,
        )
        .expect("rename comparison");
        assert_eq!(payload["summary"]["files"]["added"], 0);
        assert_eq!(payload["summary"]["files"]["removed"], 0);
        assert_eq!(payload["summary"]["files"]["renamed"], 1);
        assert_eq!(payload["file_deltas"][0]["renamed_from"], "src/old.rs");
        assert_eq!(payload["file_deltas"][0]["renamed_to"], "src/new.rs");
        assert_eq!(payload["regressions"].as_array().unwrap().len(), 0);
    }

    #[test]
    fn explicit_policy_source_selects_base_or_head_thresholds() {
        let mut base = report("standard", vec![record("src/lib.rs", 20.0)]);
        let mut changed = record("src/lib.rs", 23.0);
        changed["content_fingerprint"] = json!("changed-content");
        let mut head = report("standard", vec![changed]);
        base["config"] =
            json!({"check": {"regression_score_delta": 5.0, "fail_on_evidence_drift": false}});
        head["config"] =
            json!({"check": {"regression_score_delta": 2.0, "fail_on_evidence_drift": false}});
        let base_policy =
            compare_payload_with_policy(&base, &head, None, None, 10, false, false, "base")
                .unwrap();
        let head_policy =
            compare_payload_with_policy(&base, &head, None, None, 10, false, false, "head")
                .unwrap();
        assert_eq!(base_policy["policy_source"], "base");
        assert_eq!(base_policy["summary"]["regression_count"], 0);
        assert_eq!(head_policy["policy_source"], "head");
        assert_eq!(head_policy["summary"]["regression_count"], 1);
    }

    #[test]
    fn comparison_distinguishes_non_text_inventory_from_coverage_loss() {
        let mut binary = record("assets/image.png", 0.0);
        binary["analysis_status"] = json!("skipped");
        binary["skipped_reason"] = json!("binary");
        binary["content_fingerprint"] = json!("incomplete:binary:8");

        compare_payload_with_options(
            &report("standard", vec![binary.clone()]),
            &report("standard", vec![binary.clone()]),
            None,
            None,
            10,
            false,
            false,
        )
        .expect("non-text records are intentionally outside structural analysis");

        binary["skipped_reason"] = json!("large_file_limit");
        let error = compare_payload_with_options(
            &report("standard", vec![binary.clone()]),
            &report("standard", vec![binary]),
            None,
            None,
            10,
            false,
            false,
        )
        .expect_err("large-file coverage loss remains fail-closed");
        assert!(error.to_string().contains("inventory_evidence_incomplete"));
    }
}
