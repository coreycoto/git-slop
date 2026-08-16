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
