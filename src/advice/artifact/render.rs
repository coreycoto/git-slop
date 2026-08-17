fn string(value: Option<&Value>, fallback: &str) -> String {
    value
        .and_then(Value::as_str)
        .map(visible_controls)
        .unwrap_or_else(|| fallback.to_string())
}
fn strings(value: Option<&Value>) -> Vec<String> {
    value
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(visible_controls)
        .collect()
}

fn html_code(value: &str) -> String {
    let escaped = visible_controls(value)
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;");
    format!("<code>{escaped}</code>")
}

fn citation_lines(citations: Option<&Value>) -> Vec<String> {
    [
        ("Candidates", "candidates"),
        ("Paths", "paths"),
        ("Findings", "findings"),
        ("Relationships", "relationships"),
        ("Clusters", "clusters"),
        ("Excerpts", "excerpts"),
        ("Policies", "policies"),
        ("Verification", "verification"),
    ]
    .into_iter()
    .filter_map(|(label, key)| {
        let values = strings(citations.and_then(|value| value.get(key)));
        (!values.is_empty()).then(|| {
            format!(
                "- {label}: {}",
                values
                    .iter()
                    .map(|value| html_code(value))
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        })
    })
    .collect()
}

fn disposition(verdict: &str) -> &'static str {
    match verdict {
        "approve" => "Proceed to bounded verification before adoption.",
        "abstain" => "Pause and collect the missing evidence before deciding.",
        "revise" => "Revise the proposal and re-evaluate it before adoption.",
        "reject" => "Do not adopt the proposal as written.",
        _ => "Review the evidence before taking action.",
    }
}

pub fn render_advice_markdown(artifact: &Value) -> String {
    let candidates = artifact
        .pointer("/evaluation/candidate_evaluations")
        .and_then(Value::as_array);
    let mut verdict_counts = BTreeMap::from([
        ("approve", 0usize),
        ("abstain", 0usize),
        ("revise", 0usize),
        ("reject", 0usize),
    ]);
    let mut revision_count = 0usize;
    let mut missing_evidence_count = 0usize;
    let mut low_confidence_count = 0usize;
    for candidate in candidates.into_iter().flatten() {
        if let Some(count) = candidate
            .get("aggregate_verdict")
            .and_then(Value::as_str)
            .and_then(|verdict| verdict_counts.get_mut(verdict))
        {
            *count += 1;
        }
        revision_count += candidate
            .get("requested_revisions")
            .and_then(Value::as_array)
            .map_or(0, Vec::len);
        missing_evidence_count += candidate
            .get("missing_evidence")
            .and_then(Value::as_array)
            .map_or(0, Vec::len);
        low_confidence_count +=
            usize::from(candidate.get("confidence").and_then(Value::as_str) == Some("low"));
    }
    let aggregate = string(artifact.pointer("/evaluation/aggregate_verdict"), "unknown");
    let mut lines = vec![
        "# Git Slop policy-guided advice".to_string(),
        String::new(),
        format!(
            "> Aggregate verdict: **{}**. This output is advisory and cannot change detector truth or repository state.",
            aggregate
        ),
        String::new(),
        string(
            artifact.pointer("/evaluation/summary"),
            "No summary supplied.",
        ),
        String::new(),
        "## Decision".to_string(),
        String::new(),
        format!("- Disposition: **{}**", disposition(&aggregate)),
        format!(
            "- Candidate verdicts: {} approve, {} abstain, {} revise, {} reject",
            verdict_counts["approve"],
            verdict_counts["abstain"],
            verdict_counts["revise"],
            verdict_counts["reject"]
        ),
        format!("- Required revision items: {revision_count}"),
        format!("- Missing evidence items: {missing_evidence_count}"),
        format!("- Low-confidence candidates: {low_confidence_count}"),
        String::new(),
        "> Private retention: this artifact can contain repository-derived evidence and provider rationale. Keep `.slop/advice` Git-private and inspect retained runs with `git slop prune --dry-run`.".to_string(),
        String::new(),
        "## Provenance".to_string(),
        String::new(),
        format!(
            "- Report: `{}`",
            string(artifact.pointer("/report/sha256"), "unknown")
        ),
        format!(
            "- Revision: `{}`",
            string(artifact.pointer("/report/head_sha"), "unknown")
        ),
        format!(
            "- Context: `{}`",
            string(artifact.pointer("/context/digest"), "unknown")
        ),
        format!(
            "- Policies: `{}`",
            string(artifact.pointer("/policies/resolution_digest"), "unknown")
        ),
        format!(
            "- Provider: `{}`",
            string(artifact.pointer("/provider/provider"), "unknown")
        ),
        format!(
            "- Model: `{}`",
            string(artifact.pointer("/provider/model"), "unknown")
        ),
        format!(
            "- Endpoint class: `{}`",
            string(
                artifact.pointer("/provider/endpoint_classification"),
                "unknown"
            )
        ),
    ];
    for candidate in artifact
        .pointer("/evaluation/candidate_evaluations")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        lines.extend([
            String::new(),
            format!(
                "## {} — {}",
                string(candidate.get("candidate_id"), "candidate"),
                string(candidate.get("aggregate_verdict"), "unknown")
            ),
            String::new(),
            string(candidate.get("rationale"), "No rationale supplied."),
            String::new(),
            format!(
                "- Confidence: **{}**",
                string(candidate.get("confidence"), "unknown")
            ),
            format!(
                "- Disposition: {}",
                disposition(&string(candidate.get("aggregate_verdict"), "unknown"))
            ),
            String::new(),
            "### Evidence citations".to_string(),
            String::new(),
        ]);
        let candidate_citations = citation_lines(candidate.get("citations"));
        if candidate_citations.is_empty() {
            lines.push("- None supplied.".to_string());
        } else {
            lines.extend(candidate_citations);
        }
        lines.extend([
            String::new(),
            "### Rule evaluations".to_string(),
            String::new(),
        ]);
        for rule in candidate
            .get("rule_evaluations")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            lines.push(format!(
                "- **{} — {}:** {}",
                string(rule.get("rule_id"), "rule"),
                string(rule.get("verdict"), "unknown"),
                string(rule.get("rationale"), "No rationale supplied.")
            ));
            lines.extend(
                citation_lines(rule.get("citations"))
                    .into_iter()
                    .map(|line| format!("  {line}")),
            );
        }
        let revisions = strings(candidate.get("requested_revisions"));
        lines.extend([
            String::new(),
            "### Requested revisions".to_string(),
            String::new(),
        ]);
        if revisions.is_empty() {
            lines.push("- None.".to_string());
        } else {
            lines.extend(revisions.into_iter().map(|value| format!("- {value}")));
        }
        lines.extend([
            String::new(),
            "### Recommended next step".to_string(),
            String::new(),
            candidate
                .get("recommended_next_step")
                .and_then(Value::as_str)
                .map(visible_controls)
                .unwrap_or_else(|| {
                    "No next step was supplied; do not treat this candidate as adoption-ready."
                        .to_string()
                }),
        ]);
        let assumptions = strings(candidate.get("assumptions"));
        lines.extend([String::new(), "### Assumptions".to_string(), String::new()]);
        if assumptions.is_empty() {
            lines.push("- None.".to_string());
        } else {
            lines.extend(assumptions.into_iter().map(|value| format!("- {value}")));
        }
        let missing = strings(candidate.get("missing_evidence"));
        lines.extend([
            String::new(),
            "### Missing evidence".to_string(),
            String::new(),
        ]);
        if missing.is_empty() {
            lines.push("- None.".to_string());
        } else {
            lines.extend(missing.into_iter().map(|value| format!("- {value}")));
        }
    }
    let warnings = strings(artifact.pointer("/validation/warnings"));
    if !warnings.is_empty() {
        lines.extend([
            String::new(),
            "## Validation warnings".to_string(),
            String::new(),
        ]);
        lines.extend(warnings.into_iter().map(|value| format!("- {value}")));
    }
    lines.extend([
        String::new(),
        "---".to_string(),
        String::new(),
        string(artifact.get("boundary"), "Advice is advisory only."),
    ]);
    format!("{}\n", lines.join("\n"))
}

pub(super) fn ensure_private_directory(directory: &Path) -> Result<()> {
    if directory
        .symlink_metadata()
        .is_ok_and(|metadata| metadata.file_type().is_symlink())
    {
        bail!(
            "advice state directory must not be a symbolic link: {}",
            directory.display()
        );
    }
    fs::create_dir_all(directory)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(directory, fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}
