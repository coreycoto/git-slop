use super::super::*;
use crate::text::visible_controls;

fn string(value: Option<&Value>) -> String {
    visible_controls(&super::super::string(value))
}

fn string_or(value: Option<&Value>, fallback: &str) -> String {
    visible_controls(&super::super::string_or(value, fallback))
}

fn string_array(value: Option<&Value>) -> Vec<String> {
    super::super::string_array(value)
        .into_iter()
        .map(|value| visible_controls(&value))
        .collect()
}

fn format_reasons(value: Option<&Value>) -> String {
    let values = string_array(value);
    if values.is_empty() {
        "none".to_string()
    } else {
        values.join(", ")
    }
}

fn render_relationship_brief(value: &Value) -> String {
    format!(
        "- {} ↔ {} [{}; id={}; confidence={}; support={}; lower={:.3}; score={:.3}]",
        string(value.get("source_path")),
        string(value.get("target_path")),
        string(value.get("kind")),
        string(value.get("id")),
        string_or(value.get("confidence"), "unknown"),
        usize_value(value.get("support_count")),
        number(
            value
                .get("evidence_lower_bound")
                .or_else(|| value.get("confidence_lower_bound")),
        ),
        number(value.get("evidence_score"))
    )
}

fn render_report_context(payload: &Value) -> Vec<String> {
    let context = payload.get("report_context").unwrap_or(&Value::Null);
    let analyzer = context.get("analyzer").unwrap_or(&Value::Null);
    let characteristics = context
        .get("evidence_characteristics")
        .unwrap_or(&Value::Null);
    let target_fingerprint = string(value_at(payload, &["target", "content_fingerprint"]));
    let mut lines = vec![
        "Report and Evidence Provenance".to_string(),
        format!(
            "- analyzer: git-slop {} (analysis contract {})",
            string(analyzer.get("version")),
            json_scalar_text(analyzer.get("analysis_contract_version"))
        ),
        if target_fingerprint.is_empty() {
            format!(
                "- report_digest={} content_digest={}",
                string(context.get("report_digest")),
                string(context.get("content_digest"))
            )
        } else {
            format!(
                "- report_digest={} content_digest={} target_content_fingerprint={target_fingerprint}",
                string(context.get("report_digest")),
                string(context.get("content_digest"))
            )
        },
        format!(
            "- generated_at={} analyzed_revision_at={} head_sha={}",
            string(context.get("generated_at")),
            string(context.get("analyzed_revision_at")),
            string(context.get("head_sha"))
        ),
        format!(
            "- evidence: status={} incomplete_fields={} low_support_fields={} bounded_fields={} repository_relative={} experimental_overlays={} saturation_suppressed={}",
            string(characteristics.get("status")),
            array_at(characteristics, &["incomplete_fields"]).len(),
            array_at(characteristics, &["low_support_fields"]).len(),
            array_at(characteristics, &["bounded_fields"]).len(),
            characteristics
                .get("repository_relative")
                .and_then(Value::as_bool)
                .unwrap_or(true),
            string_array(characteristics.get("experimental_overlays")).len(),
            characteristics
                .get("saturation_suppressed")
                .and_then(Value::as_array)
                .map(Vec::len)
                .unwrap_or_default()
        ),
    ];
    let summarize_fields = |key: &str| {
        array_at(characteristics, &[key])
            .iter()
            .filter_map(|item| {
                let field = item.get("field").and_then(Value::as_str)?;
                let status = item.get("status").and_then(Value::as_str)?;
                Some(format!("{field}={status}"))
            })
            .collect::<Vec<_>>()
            .join(", ")
    };
    for (label, key) in [
        ("incomplete", "incomplete_fields"),
        ("low support", "low_support_fields"),
        ("bounded", "bounded_fields"),
    ] {
        let summary = summarize_fields(key);
        if !summary.is_empty() {
            lines.push(format!("- {label}: {summary}"));
        }
    }
    lines
}

fn render_cluster_brief(value: &Value) -> String {
    let candidate_type = string(value.get("candidate_type"));
    format!(
        "- {} [{}] members={} roots={}{}",
        string(value.get("id")),
        string(value.get("kind")),
        usize_value(value.get("member_count")).max(string_array(value.get("member_paths")).len()),
        {
            let roots = string_array(value.get("top_level_roots"));
            if roots.is_empty() {
                "none".to_string()
            } else {
                roots.join(", ")
            }
        },
        if candidate_type.is_empty() {
            String::new()
        } else {
            format!(" candidate_type={candidate_type}")
        },
    )
}

fn title_case_bool(value: bool) -> &'static str {
    if value { "True" } else { "False" }
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}

fn render_record_cost_lines(
    label: &str,
    fallback_path: &str,
    record: Option<&Value>,
) -> Vec<String> {
    let path = string(record.and_then(|value| value.get("path")));
    let path = if path.is_empty() {
        fallback_path
    } else {
        path.as_str()
    };
    let costs = record.and_then(|value| value.get("costs"));
    vec![
        format!(
            "- {label}={path} slop={} slop_score={:.1} context={}",
            string(record.and_then(|value| value.get("slop_band"))),
            number(record.and_then(|value| value.get("slop_score"))),
            string(record.and_then(|value| value.get("context_band"))),
        ),
        format!(
            "  - load: tokens={}, folder_tokens={}, pressure={:.3}",
            integer(value_at(
                costs.unwrap_or(&Value::Null),
                &["load", "file_token_count"]
            )),
            integer(value_at(
                costs.unwrap_or(&Value::Null),
                &["load", "folder_token_count"]
            )),
            number(value_at(
                costs.unwrap_or(&Value::Null),
                &["load", "load_pressure"]
            )),
        ),
        format!(
            "  - volatility: commits={}, relative_token_churn={:.3}, pressure={:.3}",
            json_scalar_text(value_at(
                costs.unwrap_or(&Value::Null),
                &["volatility", "commit_count_window"]
            )),
            number(value_at(
                costs.unwrap_or(&Value::Null),
                &["volatility", "relative_token_churn"]
            )),
            number(value_at(
                costs.unwrap_or(&Value::Null),
                &["volatility", "volatility_pressure"]
            )),
        ),
        format!(
            "  - coordination: diffusion={:.3}, degree={}, pressure={:.3}",
            number(value_at(
                costs.unwrap_or(&Value::Null),
                &["coordination", "change_diffusion"]
            )),
            json_scalar_text(value_at(
                costs.unwrap_or(&Value::Null),
                &["coordination", "cochange_degree"]
            )),
            number(value_at(
                costs.unwrap_or(&Value::Null),
                &["coordination", "coordination_pressure"]
            )),
        ),
    ]
}

fn render_overlay_lines(overlays: Option<&Value>) -> Vec<String> {
    let Some(overlays) = overlays.and_then(Value::as_object) else {
        return vec!["- none".to_string()];
    };
    if overlays.is_empty() {
        return vec!["- none".to_string()];
    }
    let organization = overlays.get("organization_health");
    let verification = overlays.get("verification");
    let navigation = overlays.get("navigation");
    let blast = overlays.get("blast_radius");
    let stewardship = overlays.get("stewardship");
    let drift = overlays.get("concept_dispersion");
    vec![
        format!(
            "- organization_health: duplication={:.3}, diffusion={:.3}, coupling={:.3}, boundary={:.3}, clusters={}",
            number(organization.and_then(|value| value.get("duplication_pressure"))),
            number(organization.and_then(|value| value.get("diffusion_pressure"))),
            number(organization.and_then(|value| value.get("coupling_pressure"))),
            number(organization.and_then(|value| value.get("boundary_pressure"))),
            string_array(organization.and_then(|value| value.get("cluster_ids"))).len(),
        ),
        format!(
            "- verification: gap={:.3}, adjacency={:.3}, test_cochange={:.3}, hotspot_without_nearby_tests={}",
            number(verification.and_then(|value| value.get("verification_gap"))),
            number(verification.and_then(|value| value.get("test_adjacency_score"))),
            number(verification.and_then(|value| value.get("test_cochange_ratio"))),
            title_case_bool(
                verification
                    .and_then(|value| value.get("hotspot_without_nearby_tests"))
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
            ),
        ),
        format!(
            "- navigation: pressure={:.3}, ambiguity={:.3}, path_depth={}",
            number(navigation.and_then(|value| value.get("navigation_pressure"))),
            number(navigation.and_then(|value| value.get("search_ambiguity"))),
            json_scalar_text(navigation.and_then(|value| value.get("path_depth"))),
        ),
        format!(
            "- blast_radius: pressure={:.3}, degree={}, cross_folder={:.3}",
            number(blast.and_then(|value| value.get("blast_radius_pressure"))),
            json_scalar_text(blast.and_then(|value| value.get("cochange_degree"))),
            number(blast.and_then(|value| value.get("cross_folder_coupling"))),
        ),
        format!(
            "- stewardship: pressure={:.3}, authors={}, top_author_share={:.3}",
            number(stewardship.and_then(|value| value.get("stewardship_pressure"))),
            json_scalar_text(stewardship.and_then(|value| value.get("author_count_window"))),
            number(stewardship.and_then(|value| value.get("top_author_share"))),
        ),
        format!(
            "- concept_dispersion: pressure={:.3}, terms={}",
            number(drift.and_then(|value| value.get("concept_dispersion_pressure"))),
            {
                let terms = string_array(drift.and_then(|value| {
                    value
                        .get("dispersed_terms")
                        .or_else(|| value.get("drift_terms"))
                }));
                if terms.is_empty() {
                    "none".to_string()
                } else {
                    terms.into_iter().take(5).collect::<Vec<_>>().join(", ")
                }
            },
        ),
    ]
}

fn render_evidence_summary(payload: &Value) -> Vec<String> {
    let summary = payload.get("evidence_summary").unwrap_or(&Value::Null);
    let joined = |key: &str| {
        let values = string_array(summary.get(key));
        if values.is_empty() {
            "none".to_string()
        } else {
            values.join("; ")
        }
    };
    let relationships = string_array(value_at(
        summary,
        &["supporting_evidence", "relationship_ids"],
    ));
    let clusters = string_array(value_at(summary, &["supporting_evidence", "cluster_ids"]));
    vec![
        "Evidence Summary".to_string(),
        format!("- strongest detector costs: {}", joined("detector_cost")),
        format!("- strongest overlays: {}", joined("strongest_overlays")),
        format!(
            "- supporting relationships: {}",
            if relationships.is_empty() {
                "none".to_string()
            } else {
                relationships.join(", ")
            }
        ),
        format!(
            "- supporting clusters: {}",
            if clusters.is_empty() {
                "none".to_string()
            } else {
                clusters.join(", ")
            }
        ),
        format!(
            "- interpretation: {}",
            string(summary.get("interpretation"))
        ),
    ]
}

fn render_top_explain(payload: &Value) -> String {
    let count = usize_value(value_at(payload, &["target", "count"]));
    let requested = usize_value(value_at(payload, &["target", "requested_count"]));
    if count == 0 {
        return format!(
            "Explain: no matching hotspots\n\nNo action-queue hotspots were present in this report (requested {requested}).\n"
        );
    }
    let title = if requested > count {
        format!("Explain: top {count} hotspots (requested {requested})")
    } else {
        format!("Explain: top {count} hotspots")
    };
    let mut lines = vec![title, String::new()];
    for (index, item) in array_at(payload, &["items"]).iter().enumerate() {
        let target = item.get("target").unwrap_or(&Value::Null);
        lines.push(format!(
            "{}. {} [{}] slop_score={:.1} context={}",
            index + 1,
            string(target.get("path")),
            string(target.get("slop_band")),
            number(target.get("slop_score")),
            string(target.get("context_band"))
        ));
        lines.push(format!(
            "   cost: load={:.3} volatility={:.3} coordination={:.3}",
            number(value_at(item, &["cost_summary", "load", "load_pressure"])),
            number(value_at(
                item,
                &["cost_summary", "volatility", "volatility_pressure"]
            )),
            number(value_at(
                item,
                &["cost_summary", "coordination", "coordination_pressure"]
            ))
        ));
        let pressures = strongest_pressures(item.get("overlay_summary"), 3);
        lines.push(format!(
            "   overlays: {}",
            if pressures.is_empty() {
                "none".to_string()
            } else {
                pressures
                    .iter()
                    .map(|(label, value)| format!("{label}={value:.3}"))
                    .collect::<Vec<_>>()
                    .join(", ")
            }
        ));
        let relationships: Vec<String> = array_at(item, &["supporting_relationships"])
            .iter()
            .take(2)
            .map(|value| {
                format!(
                    "{} ↔ {} [{}]",
                    string(value.get("source_path")),
                    string(value.get("target_path")),
                    string(value.get("kind"))
                )
            })
            .collect();
        let clusters: Vec<String> = array_at(item, &["supporting_clusters"])
            .iter()
            .take(2)
            .map(|value| string(value.get("id")))
            .collect();
        lines.push(format!(
            "   relationships: {}",
            if relationships.is_empty() {
                "none".to_string()
            } else {
                relationships.join(", ")
            }
        ));
        lines.push(format!(
            "   clusters: {}",
            if clusters.is_empty() {
                "none".to_string()
            } else {
                clusters.join(", ")
            }
        ));
        lines.push(String::new());
    }
    lines.extend(render_evidence_summary(payload));
    lines.push(String::new());
    lines.extend(render_report_context(payload));
    lines.push(String::new());
    lines.push(string(payload.get("boundary_note")));
    format!("{}\n", lines.join("\n"))
}

pub fn render_explain_summary_text(payload: &Value) -> String {
    if string(value_at(payload, &["selector", "kind"])) == "top" {
        let items = array_at(payload, &["items"]);
        if items.is_empty() {
            return "Explain: no matching hotspots\n\nNo action-queue hotspots were present in this report.\n".to_string();
        }
        let mut lines = vec!["Why these hotspots matter".to_string(), String::new()];
        for (index, item) in items.iter().enumerate() {
            let target = item.get("target").unwrap_or(&Value::Null);
            lines.push(format!(
                "{}. {} — {}",
                index + 1,
                string(target.get("path")),
                string_or(
                    value_at(item, &["evidence_summary", "interpretation"]),
                    "This path combines enough repository cost signals to merit review."
                )
            ));
        }
        lines.push(String::new());
        lines.push(
            "Use `git slop explain --path <path> --verbose` for normalized metrics and provenance."
                .to_string(),
        );
        lines.push(String::new());
        lines.push(string(payload.get("boundary_note")));
        return format!("{}\n", lines.join("\n"));
    }

    let target = payload.get("target").unwrap_or(&Value::Null);
    let kind = string(target.get("kind"));
    let label = if kind == "path" {
        string(target.get("path"))
    } else {
        string(target.get("id"))
    };
    let interpretation = string_or(
        value_at(payload, &["evidence_summary", "interpretation"]),
        "The report found structural evidence worth a bounded maintainer review.",
    );
    let mut lines = vec![
        format!("Why this matters: {label}"),
        String::new(),
        interpretation,
        String::new(),
        "What the report observed".to_string(),
    ];
    match kind.as_str() {
        "path" => lines.push(format!(
            "- Maintenance pressure is {} and context load is {}.",
            string_or(target.get("slop_band"), "not classified"),
            string_or(target.get("context_band"), "not classified")
        )),
        "relationship" => lines.push(format!(
            "- {} and {} have a {} relationship supported by repository history or structure.",
            string(target.get("source_path")),
            string(target.get("target_path")),
            string_or(target.get("relationship_kind"), "recorded")
        )),
        "cluster" => lines.push(format!(
            "- The {} cluster groups {} related paths for joint review.",
            string_or(target.get("cluster_kind"), "recorded"),
            string_array(target.get("member_paths")).len()
        )),
        _ => {
            lines.push("- The selected report record has bounded supporting evidence.".to_string())
        }
    }
    lines.push(String::new());
    lines.push("Suggested next step".to_string());
    match kind.as_str() {
        "path" => lines.push(format!(
            "- Draft a bounded proposal: git slop plan --path {}",
            shell_quote(&label)
        )),
        "relationship" => lines.push(format!(
            "- Draft a bounded proposal: git slop plan --relationship {}",
            shell_quote(&label)
        )),
        "cluster" => lines.push(format!(
            "- Draft a bounded proposal: git slop plan --cluster {}",
            shell_quote(&label)
        )),
        _ => {}
    }
    lines.push("- Show raw evidence: rerun this command with `--verbose`.".to_string());
    lines.push(String::new());
    lines.push(string(payload.get("boundary_note")));
    format!("{}\n", lines.join("\n"))
}

pub fn render_explain_text(payload: &Value) -> String {
    if string(value_at(payload, &["selector", "kind"])) == "top" {
        return render_top_explain(payload);
    }
    let target = payload.get("target").unwrap_or(&Value::Null);
    let target_kind = string(target.get("kind"));
    let target_label = match target_kind.as_str() {
        "path" => string(target.get("path")),
        _ => string(target.get("id")),
    };
    let target_detail = match target_kind.as_str() {
        "path" => string(target.get("record_type")),
        "relationship" => string(target.get("relationship_kind")),
        "cluster" => string(target.get("cluster_kind")),
        _ => String::new(),
    };
    let mut lines = vec![
        format!("Explain: {target_kind} {target_label} [{target_detail}]"),
        String::new(),
    ];

    if target_kind == "relationship" {
        lines.push("Hotspot Cost".to_string());
        lines.extend(render_record_cost_lines(
            "source",
            &string(target.get("source_path")),
            value_at(payload, &["cost_summary", "source"]),
        ));
        lines.extend(render_record_cost_lines(
            "target",
            &string(target.get("target_path")),
            value_at(payload, &["cost_summary", "target"]),
        ));
        lines.push(String::new());
        lines.push("Overlay Evidence".to_string());
        let organization = value_at(payload, &["overlay_summary", "organization_health"]);
        lines.push(format!(
            "- organization_health: confidence={}, support={}, evidence_lower_bound={:.3}, evidence_score={:.3}, crosses_top_level_boundary={}",
            string_or(
                organization.and_then(|value| value.get("confidence")),
                "unknown"
            ),
            usize_value(organization.and_then(|value| value.get("support_count"))),
            number(organization.and_then(|value| {
                value
                    .get("evidence_lower_bound")
                    .or_else(|| value.get("confidence_lower_bound"))
            })),
            number(organization.and_then(|value| value.get("evidence_score"))),
            title_case_bool(
                organization
                    .and_then(|value| value.get("crosses_top_level_boundary"))
                    .and_then(Value::as_bool)
                    .unwrap_or(false)
            ),
        ));
        lines.push("- source overlays:".to_string());
        lines.extend(render_overlay_lines(value_at(
            payload,
            &["overlay_summary", "source_overlays"],
        )));
        lines.push("- target overlays:".to_string());
        lines.extend(render_overlay_lines(value_at(
            payload,
            &["overlay_summary", "target_overlays"],
        )));
    } else if target_kind == "cluster" {
        lines.push("Hotspot Cost".to_string());
        let roots = string_array(target.get("top_level_roots"));
        lines.push(format!(
            "- members={} roots={} candidate_type={}",
            usize_value(value_at(payload, &["cost_summary", "member_count"])),
            if roots.is_empty() {
                "none".to_string()
            } else {
                roots.join(", ")
            },
            string_or(target.get("candidate_type"), "n/a"),
        ));
        lines.push("- member hotspots:".to_string());
        for item in array_at(payload, &["cost_summary", "member_hotspots"]) {
            lines.push(format!(
                "  - {} slop={} slop_score={:.1} context={}",
                string(item.get("path")),
                string(item.get("slop_band")),
                number(item.get("slop_score")),
                string(item.get("context_band"))
            ));
        }
        lines.push(String::new());
        lines.push("Overlay Evidence".to_string());
        let organization = value_at(payload, &["overlay_summary", "organization_health"]);
        lines.push(format!(
            "- organization_health: candidate_type={}, evidence_score={:.3}",
            string_or(
                organization.and_then(|value| value.get("candidate_type")),
                &string(organization.and_then(|value| value.get("kind"))),
            ),
            number(organization.and_then(|value| value.get("evidence_score"))),
        ));
        let maxima = value_at(payload, &["overlay_summary", "member_overlay_maxima"]);
        if maxima.is_some() {
            lines.push(format!(
                "- member overlay maxima: organization.diffusion={:.3}, verification={:.3}, navigation={:.3}, blast_radius={:.3}, concept_dispersion={:.3}",
                number(value_at(maxima.unwrap_or(&Value::Null), &["organization_health", "diffusion_pressure"])),
                number(value_at(maxima.unwrap_or(&Value::Null), &["verification", "verification_gap"])),
                number(value_at(maxima.unwrap_or(&Value::Null), &["navigation", "navigation_pressure"])),
                number(value_at(maxima.unwrap_or(&Value::Null), &["blast_radius", "blast_radius_pressure"])),
                number(value_at(maxima.unwrap_or(&Value::Null), &["concept_dispersion", "concept_dispersion_pressure"])),
            ));
        }
    } else {
        let is_folder = string(target.get("record_type")) == "folder";
        lines.push("Hotspot Cost".to_string());
        lines.push(format!(
            "- slop: {} ({:.1}) context={} reasons={}",
            string(target.get("slop_band")),
            number(target.get("slop_score")),
            string(target.get("context_band")),
            format_reasons(target.get("reason_codes"))
        ));
        lines.push(format!(
            "- load: {}={}, folder_tokens={}, pressure={:.3}",
            if is_folder {
                "max_file_tokens"
            } else {
                "tokens"
            },
            integer(value_at(
                payload,
                &["cost_summary", "load", "file_token_count"]
            )),
            integer(value_at(
                payload,
                &["cost_summary", "load", "folder_token_count"]
            )),
            number(value_at(
                payload,
                &["cost_summary", "load", "load_pressure"]
            ))
        ));
        lines.push(format!(
            "- volatility: commits={}, relative_token_churn={:.3}, pressure={:.3}",
            json_scalar_text(value_at(
                payload,
                &["cost_summary", "volatility", "commit_count_window"]
            )),
            number(value_at(
                payload,
                &["cost_summary", "volatility", "relative_token_churn"]
            )),
            number(value_at(
                payload,
                &["cost_summary", "volatility", "volatility_pressure"]
            ))
        ));
        lines.push(format!(
            "- coordination: diffusion={:.3}, degree={}, pressure={:.3}",
            number(value_at(
                payload,
                &["cost_summary", "coordination", "change_diffusion"]
            )),
            value_at(
                payload,
                &["cost_summary", "coordination", "cochange_degree"]
            )
            .map(ToString::to_string)
            .unwrap_or_else(|| "0".to_string()),
            number(value_at(
                payload,
                &["cost_summary", "coordination", "coordination_pressure"]
            ))
        ));
        let descendants = array_at(payload, &["cost_summary", "descendant_hotspots"]);
        if is_folder {
            lines.push("- descendant hotspots:".to_string());
            if descendants.is_empty() {
                lines.push("  - none".to_string());
            } else {
                for record in descendants.iter().take(5) {
                    lines.push(format!(
                        "  - {} slop={} slop_score={:.1} context={}",
                        string(record.get("path")),
                        string(record.get("slop_band")),
                        number(record.get("slop_score")),
                        string(record.get("context_band"))
                    ));
                }
            }
        }
        lines.push(String::new());
        lines.push("Overlay Evidence".to_string());
        lines.extend(render_overlay_lines(payload.get("overlay_summary")));
        if is_folder
            && payload
                .pointer("/overlay_summary/descendant_overlay_maxima")
                .is_some()
        {
            lines.push(format!(
                "- descendant overlay maxima: organization.diffusion={:.3}, verification={:.3}, navigation={:.3}, blast_radius={:.3}, concept_dispersion={:.3}",
                number(value_at(payload, &["overlay_summary", "descendant_overlay_maxima", "organization_health", "diffusion_pressure"])),
                number(value_at(payload, &["overlay_summary", "descendant_overlay_maxima", "verification", "verification_gap"])),
                number(value_at(payload, &["overlay_summary", "descendant_overlay_maxima", "navigation", "navigation_pressure"])),
                number(value_at(payload, &["overlay_summary", "descendant_overlay_maxima", "blast_radius", "blast_radius_pressure"])),
                number(value_at(payload, &["overlay_summary", "descendant_overlay_maxima", "concept_dispersion", "concept_dispersion_pressure"])),
            ));
        }
    }
    lines.push(String::new());
    lines.push("Supporting Relationships".to_string());
    let relationships = array_at(payload, &["supporting_relationships"]);
    let path_is_folder = target_kind == "path" && string(target.get("record_type")) == "folder";
    let relationship_limit = if path_is_folder {
        3
    } else {
        relationships.len()
    };
    if relationships.is_empty() {
        lines.push("- none".to_string());
    } else {
        lines.extend(
            relationships
                .iter()
                .take(relationship_limit)
                .map(render_relationship_brief),
        );
    }
    lines.push(String::new());
    lines.push("Supporting Clusters".to_string());
    let clusters = array_at(payload, &["supporting_clusters"]);
    let cluster_limit = if path_is_folder { 3 } else { clusters.len() };
    if clusters.is_empty() {
        lines.push("- none".to_string());
    } else {
        lines.extend(
            clusters
                .iter()
                .take(cluster_limit)
                .map(render_cluster_brief),
        );
    }
    lines.push(String::new());
    lines.extend(render_evidence_summary(payload));
    lines.push(String::new());
    lines.extend(render_report_context(payload));
    if target_kind == "relationship" {
        lines.push(String::new());
        lines.push("Next Commands".to_string());
        lines.push(format!(
            "- Inspect source: git-slop show {}",
            shell_quote(&string(target.get("source_path")))
        ));
        lines.push(format!(
            "- Inspect target: git-slop show {}",
            shell_quote(&string(target.get("target_path")))
        ));
        lines.push(format!(
            "- Draft a bounded plan: git slop plan --relationship {}",
            shell_quote(&string(target.get("id")))
        ));
    } else if target_kind == "cluster" {
        lines.push(String::new());
        lines.push("Next Commands".to_string());
        for member in string_array(target.get("member_paths"))
            .into_iter()
            .take(MAX_SLICE_FILES)
        {
            lines.push(format!(
                "- Inspect member: git-slop show {}",
                shell_quote(&member)
            ));
        }
        lines.push(format!(
            "- Draft a bounded plan: git slop plan --cluster {}",
            shell_quote(&string(target.get("id")))
        ));
    }
    lines.push(String::new());
    lines.push(string(payload.get("boundary_note")));
    format!("{}\n", lines.join("\n"))
}
