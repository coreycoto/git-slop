fn array_at<'a>(value: &'a Value, path: &[&str]) -> &'a [Value] {
    let mut current = value;
    for key in path {
        let Some(next) = current.get(*key) else {
            return &[];
        };
        current = next;
    }
    current.as_array().map(Vec::as_slice).unwrap_or(&[])
}

fn value_at<'a>(value: &'a Value, path: &[&str]) -> Option<&'a Value> {
    let mut current = value;
    for key in path {
        current = current.get(*key)?;
    }
    Some(current)
}

fn string(value: Option<&Value>) -> String {
    value
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

fn string_or(value: Option<&Value>, fallback: &str) -> String {
    value
        .and_then(Value::as_str)
        .unwrap_or(fallback)
        .to_string()
}

fn number(value: Option<&Value>) -> f64 {
    value.and_then(Value::as_f64).unwrap_or(0.0)
}

fn round6(value: f64) -> f64 {
    (value * 1_000_000.0).round() / 1_000_000.0
}

fn optional_string(value: Option<&Value>) -> Option<String> {
    value.and_then(Value::as_str).map(ToOwned::to_owned)
}

fn integer(value: Option<&Value>) -> i64 {
    value
        .and_then(|value| {
            value
                .as_i64()
                .or_else(|| value.as_u64().and_then(|value| i64::try_from(value).ok()))
                .or_else(|| value.as_f64().map(|value| value as i64))
        })
        .unwrap_or(0)
}

fn usize_value(value: Option<&Value>) -> usize {
    value
        .and_then(Value::as_u64)
        .and_then(|value| usize::try_from(value).ok())
        .unwrap_or(0)
}

fn string_array(value: Option<&Value>) -> Vec<String> {
    value
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(ToOwned::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

fn cmp_f64_desc(left: f64, right: f64) -> Ordering {
    right.partial_cmp(&left).unwrap_or(Ordering::Equal)
}

fn evidence_then_id(left: &Value, right: &Value) -> Ordering {
    cmp_f64_desc(
        number(left.get("evidence_score")),
        number(right.get("evidence_score")),
    )
    .then_with(|| string(left.get("id")).cmp(&string(right.get("id"))))
}

fn normalized_path(path: &str) -> String {
    let trimmed = path.trim();
    if trimmed.is_empty() || trimmed == "." {
        ".".to_string()
    } else {
        trimmed.trim_end_matches('/').to_string()
    }
}

fn path_matches_folder(path: &str, folder: &str) -> bool {
    folder == "." || path.starts_with(&format!("{}/", folder.trim_end_matches('/')))
}

fn report_schema(report: &Value) -> i64 {
    integer(report.get("schema_version"))
}

fn require_report_schema(report: &Value, command: &str) -> Result<()> {
    if report_schema(report) != REPORT_SCHEMA_VERSION {
        bail!("git slop {command} requires report schema {REPORT_SCHEMA_VERSION}.");
    }
    Ok(())
}

fn relationship_sections(report: &Value, canonical_first: bool) -> &Value {
    let top_level = report.get("relationships").filter(|value| {
        value
            .as_object()
            .map(|value| !value.is_empty())
            .unwrap_or(false)
    });
    let canonical = value_at(
        report,
        &["overlays", "organization_health", "relationships"],
    )
    .filter(|value| {
        value
            .as_object()
            .map(|value| !value.is_empty())
            .unwrap_or(false)
    });
    if canonical_first {
        canonical.or(top_level).unwrap_or(&Value::Null)
    } else {
        top_level.or(canonical).unwrap_or(&Value::Null)
    }
}

fn cluster_sections(report: &Value, canonical_first: bool) -> &Value {
    let top_level = report.get("clusters").filter(|value| {
        value
            .as_object()
            .map(|value| !value.is_empty())
            .unwrap_or(false)
    });
    let canonical =
        value_at(report, &["overlays", "organization_health", "clusters"]).filter(|value| {
            value
                .as_object()
                .map(|value| !value.is_empty())
                .unwrap_or(false)
        });
    if canonical_first {
        canonical.or(top_level).unwrap_or(&Value::Null)
    } else {
        top_level.or(canonical).unwrap_or(&Value::Null)
    }
}

fn all_relationships(report: &Value, canonical_first: bool) -> Vec<Value> {
    let sections = relationship_sections(report, canonical_first);
    let mut result = Vec::new();
    for key in RELATIONSHIP_KEYS {
        result.extend(array_at(sections, &[key]).iter().cloned());
    }
    result.sort_by(evidence_then_id);
    dedupe_by_id(result)
}

fn all_clusters(report: &Value, canonical_first: bool) -> Vec<Value> {
    let sections = cluster_sections(report, canonical_first);
    let mut result = Vec::new();
    for key in CLUSTER_KEYS {
        result.extend(array_at(sections, &[key]).iter().cloned());
    }
    result.sort_by(evidence_then_id);
    // A consolidation-candidate mirror may intentionally reuse its source
    // cluster ID. Preserve memberships across cluster kinds; ID-only lookup
    // remains deterministic because section order is stable.
    result
}

fn dedupe_by_id(items: Vec<Value>) -> Vec<Value> {
    let mut seen = BTreeSet::new();
    items
        .into_iter()
        .filter(|item| {
            let id = string(item.get("id"));
            !id.is_empty() && seen.insert(id)
        })
        .collect()
}

fn matching_relationships(report: &Value, target: &str, folder: bool) -> Vec<Value> {
    let mut result: Vec<Value> = all_relationships(report, false)
        .into_iter()
        .filter(|item| {
            let source = string(item.get("source_path"));
            let target_path = string(item.get("target_path"));
            if folder {
                path_matches_folder(&source, target) || path_matches_folder(&target_path, target)
            } else {
                source == target || target_path == target
            }
        })
        .collect();
    result.sort_by(evidence_then_id);
    result
}

fn matching_clusters(report: &Value, target: &str, folder: bool) -> Vec<Value> {
    let mut result: Vec<Value> = all_clusters(report, false)
        .into_iter()
        .filter(|item| {
            string_array(item.get("member_paths")).iter().any(|member| {
                if folder {
                    path_matches_folder(member, target)
                } else {
                    member == target
                }
            })
        })
        .collect();
    result.sort_by(evidence_then_id);
    result
}

fn find_record(report: &Value, target: &str) -> Option<(Value, bool)> {
    let target = normalized_path(target);
    for record in array_at(report, &["files"]) {
        if string(record.get("path")) == target {
            return Some((record.clone(), true));
        }
    }
    for record in array_at(report, &["folders"]) {
        if string(record.get("path")) == target {
            return Some((record.clone(), false));
        }
    }
    None
}

pub fn show_payload(report: &Value, target: &str) -> Option<Value> {
    let target = normalized_path(target);
    let (record, is_file) = find_record(report, &target)?;
    let mut payload = record.as_object()?.clone();
    let overlays = payload.get("overlays").cloned().unwrap_or(Value::Null);
    payload.insert(
        "record_type".to_string(),
        Value::String(if is_file { "file" } else { "folder" }.to_string()),
    );
    payload.insert(
        "organization_health".to_string(),
        overlays
            .get("organization_health")
            .cloned()
            .unwrap_or(Value::Null),
    );
    payload.insert(
        "strongest_relationships".to_string(),
        Value::Array(
            matching_relationships(report, &target, !is_file)
                .into_iter()
                .take(10)
                .collect(),
        ),
    );
    payload.insert(
        "cluster_memberships".to_string(),
        Value::Array(
            matching_clusters(report, &target, !is_file)
                .into_iter()
                .take(10)
                .collect(),
        ),
    );
    Some(Value::Object(payload))
}

pub fn render_show_text(payload: &Value) -> String {
    let kind = string_or(payload.get("record_type"), "record");
    let path = visible_controls(&string(payload.get("path")));
    let mut lines = vec![format!(
        "{}: {}",
        if kind == "file" { "File" } else { "Folder" },
        path
    )];
    lines.push(format!(
        "tokens={} context={} slop={} score={:.1}",
        integer(payload.get("tokens")),
        string_or(payload.get("context_band"), "unknown"),
        string_or(payload.get("slop_band"), "unknown"),
        number(payload.get("slop_score")),
    ));
    let reasons = string_array(payload.get("reason_codes"));
    if !reasons.is_empty() {
        lines.push(format!("reasons: {}", reasons.join(", ")));
    }
    let relationships = array_at(payload, &["strongest_relationships"]);
    if !relationships.is_empty() {
        lines.push("relationships:".to_string());
        for item in relationships.iter().take(5) {
            lines.push(format!(
                "- {} ↔ {} kind={} confidence={} lower={:.3} support={} evidence={:.3} id={}",
                visible_controls(&string(item.get("source_path"))),
                visible_controls(&string(item.get("target_path"))),
                string(item.get("kind")),
                string_or(item.get("confidence"), "unknown"),
                number(
                    item.get("evidence_lower_bound")
                        .or_else(|| item.get("confidence_lower_bound")),
                ),
                integer(item.get("support_count")),
                number(item.get("evidence_score")),
                visible_controls(&string(item.get("id"))),
            ));
        }
    }
    lines.push(format!("next: git slop explain --path {}", path));
    lines.join("\n") + "\n"
}
