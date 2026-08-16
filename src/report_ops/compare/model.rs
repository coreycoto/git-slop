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
