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
