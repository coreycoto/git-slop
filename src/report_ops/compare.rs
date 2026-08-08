use super::*;

fn optional_number(value: Option<&Value>) -> Option<f64> {
    value.and_then(Value::as_f64).map(round6)
}

fn optional_integer(value: Option<&Value>) -> Option<i64> {
    value.and_then(Value::as_i64)
}

fn records_by_path(report: &Value, collection: &str) -> BTreeMap<String, Value> {
    array_at(report, &[collection])
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
        "critical" | "refactor_required" => 3,
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

fn record_delta_status(base: Option<&Value>, head: Option<&Value>) -> &'static str {
    match (base, head) {
        (None, Some(_)) => "added",
        (Some(_), None) => "removed",
        (None, None) => "unchanged",
        (Some(base), Some(head))
            if record_score(Some(base)) == record_score(Some(head))
                && record_tokens(Some(base)) == record_tokens(Some(head))
                && record_load_pressure(Some(base)) == record_load_pressure(Some(head))
                && record_band(Some(base), "context_band")
                    == record_band(Some(head), "context_band")
                && record_band(Some(base), "slop_band") == record_band(Some(head), "slop_band")
                && record_overlay_delta(Some(base), Some(head)).is_empty() =>
        {
            "unchanged"
        }
        _ => "changed",
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

fn require_compatible_reports(base: &Value, head: &Value) -> Result<()> {
    for (label, pointer) in [
        ("repository identity", "/repo/remote_url"),
        ("tokenizer", "/analyzer/context_tokenizer"),
        ("configuration digest", "/analyzer/config_digest"),
        ("analyzer version", "/analyzer/version"),
        ("history completeness", "/stats/history_complete"),
    ] {
        let left = compatibility_value(base, pointer);
        let right = compatibility_value(head, pointer);
        if left != right {
            bail!(
                "reports have incompatible {label}; rerun compare with --force only if this mismatch is intentional"
            );
        }
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
    let context_delta = band_delta(base_context.as_deref(), head_context.as_deref());
    let slop_delta = band_delta(base_slop.as_deref(), head_slop.as_deref());
    json!({
        "path": path,
        "status": record_delta_status(base, head),
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
        "overlay_deltas": record_overlay_delta(base, head),
    })
}

fn build_record_deltas(base: &Value, head: &Value, collection: &str) -> Vec<Value> {
    let base_records = records_by_path(base, collection);
    let head_records = records_by_path(head, collection);
    let paths: BTreeSet<String> = base_records
        .keys()
        .chain(head_records.keys())
        .cloned()
        .collect();
    paths
        .into_iter()
        .map(|path| build_record_delta(&path, base_records.get(&path), head_records.get(&path)))
        .collect()
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
        "changed": count("changed"),
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
    json!({
        "path": path,
        "repo_name": repo.get("repo_name").cloned().unwrap_or(Value::Null),
        "head_sha": repo.get("head_sha").or_else(|| repo.get("head_commit")).cloned().unwrap_or(Value::Null),
        "generated_at": report.get("generated_at")
            .or_else(|| value_at(report, &["summary", "generated_at"]))
            .cloned()
            .unwrap_or(Value::Null),
        "schema_version": report.get("schema_version").cloned().unwrap_or(Value::Null),
    })
}

pub fn compare_payload(
    base_report: &Value,
    head_report: &Value,
    base_path: Option<&str>,
    head_path: Option<&str>,
    top: usize,
) -> Result<Value> {
    compare_payload_with_force(base_report, head_report, base_path, head_path, top, false)
}

pub fn compare_payload_with_force(
    base_report: &Value,
    head_report: &Value,
    base_path: Option<&str>,
    head_path: Option<&str>,
    top: usize,
    force: bool,
) -> Result<Value> {
    if report_schema(base_report) != REPORT_SCHEMA_VERSION {
        bail!("base report must use schema {REPORT_SCHEMA_VERSION}.");
    }
    if report_schema(head_report) != REPORT_SCHEMA_VERSION {
        bail!("head report must use schema {REPORT_SCHEMA_VERSION}.");
    }
    if top == 0 {
        bail!("--top must be greater than zero.");
    }
    if !force {
        require_compatible_reports(base_report, head_report)?;
    }
    let file_deltas = build_record_deltas(base_report, head_report, "files");
    let folder_deltas = build_record_deltas(base_report, head_report, "folders");
    let worsened = file_deltas
        .iter()
        .filter(|item| {
            string(item.get("status")) == "changed" && number(item.get("slop_score_delta")) > 0.0
        })
        .count();
    let improved = file_deltas
        .iter()
        .filter(|item| {
            string(item.get("status")) == "changed" && number(item.get("slop_score_delta")) < 0.0
        })
        .count();
    let mut queue_movement = build_queue_movement(base_report, head_report);
    queue_movement.truncate(top);
    let overlay_deltas = aggregate_overlay_deltas(&file_deltas);
    Ok(json!({
        "schema_version": COMPARE_SCHEMA_VERSION,
        "report_schema_version": REPORT_SCHEMA_VERSION,
        "command": "compare",
        "base_report": report_descriptor(base_report, base_path),
        "head_report": report_descriptor(head_report, head_path),
        "summary": {
            "files": delta_counts(&file_deltas),
            "folders": delta_counts(&folder_deltas),
            "worsened_file_count": worsened,
            "improved_file_count": improved,
        },
        "file_deltas": file_deltas,
        "folder_deltas": folder_deltas,
        "queue_movement": queue_movement,
        "overlay_deltas": overlay_deltas,
        "boundary_note": COMPARE_BOUNDARY_NOTE,
        "compatibility_forced": force,
    }))
}

fn file_name_or_fallback(value: &str, fallback: &str) -> String {
    std::path::Path::new(value)
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or(fallback)
        .to_string()
}

pub fn render_compare_text(payload: &Value, top: usize) -> String {
    let base_path = string(value_at(payload, &["base_report", "path"]));
    let head_path = string(value_at(payload, &["head_report", "path"]));
    let base_name = file_name_or_fallback(
        if base_path.is_empty() {
            "<base>"
        } else {
            &base_path
        },
        "<base>",
    );
    let head_name = file_name_or_fallback(
        if head_path.is_empty() {
            "<head>"
        } else {
            &head_path
        },
        "<head>",
    );
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
                string(item.get("path")),
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
                string(item.get("path")),
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
                string(item.get("path")),
                string(item.get("status")),
                json_scalar_text(item.get("base_position")),
                json_scalar_text(item.get("head_position")),
            )
        }));
    }
    lines.extend([String::new(), string(payload.get("boundary_note"))]);
    lines.join("\n")
}
