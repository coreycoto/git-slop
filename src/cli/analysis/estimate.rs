fn print_find_estimate(
    payload: &Value,
    normalized_scope: Option<&str>,
    format: DisplayFormat,
) -> Result<()> {
    match format {
        DisplayFormat::Json => print_text(&render_json(payload)?),
        DisplayFormat::Yaml => print_text(&serde_yaml::to_string(payload)?),
        DisplayFormat::Text => print_text(&render_find_estimate_text(payload, normalized_scope)),
    }
    Ok(())
}

fn render_find_estimate_text(payload: &Value, normalized_scope: Option<&str>) -> String {
    let estimate = &payload["estimate"];
    let mib = |key: &str| {
        estimate[key]
            .as_u64()
            .unwrap_or_default()
            .div_ceil(1024 * 1024)
    };
    [
        "Git Slop scan estimate".to_string(),
        format!(
            "- scope: {}",
            normalized_scope.unwrap_or("all tracked paths")
        ),
        format!("- tracked paths: {}", estimate["tracked_path_count"]),
        format!(
            "- peak memory: ~{} MiB ({}-{} MiB; {} MiB budget)",
            mib("estimated_peak_memory_bytes"),
            mib("estimated_peak_memory_low_bytes"),
            mib("estimated_peak_memory_high_bytes"),
            mib("memory_budget_bytes")
        ),
        format!(
            "- cache/report: ~{} MiB / ~{} MiB",
            mib("estimated_cache_bytes"),
            mib("estimated_report_bytes")
        ),
        format!(
            "- time: ~{}s cold / ~{}s warm",
            estimate["estimated_seconds_cold"], estimate["estimated_seconds_warm"]
        ),
        format!("- inodes: ~{}", estimate["estimated_inode_count"]),
        format!(
            "- cache assumptions: {}",
            estimate["cache_assumptions"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .collect::<Vec<_>>()
                .join("; ")
        ),
        format!(
            "- confidence: {}",
            estimate["confidence"].as_str().unwrap_or("unknown")
        ),
        "Next: run `git slop find` when the estimate fits your budget.".to_string(),
    ]
    .join("\n")
}

#[cfg(test)]
mod estimate_render_tests {
    use super::*;

    #[test]
    fn text_receipt_rounds_bytes_up_and_keeps_scope_and_assumptions() {
        let payload = json!({
            "estimate": {
                "tracked_path_count": 12,
                "estimated_peak_memory_bytes": 1_048_577,
                "estimated_peak_memory_low_bytes": 1,
                "estimated_peak_memory_high_bytes": 2_097_153,
                "memory_budget_bytes": 4_194_304,
                "estimated_cache_bytes": 0,
                "estimated_report_bytes": 1_048_576,
                "estimated_seconds_cold": 8,
                "estimated_seconds_warm": 3,
                "estimated_inode_count": 42,
                "cache_assumptions": ["cold cache", "complete history"],
                "confidence": "conservative"
            }
        });
        let rendered = render_find_estimate_text(&payload, Some("src/core"));
        for expected in [
            "- scope: src/core",
            "- tracked paths: 12",
            "- peak memory: ~2 MiB (1-3 MiB; 4 MiB budget)",
            "- cache/report: ~0 MiB / ~1 MiB",
            "- cache assumptions: cold cache; complete history",
            "- confidence: conservative",
        ] {
            assert!(rendered.contains(expected), "missing {expected:?}\n{rendered}");
        }
    }

    #[test]
    fn text_receipt_has_safe_fallbacks_for_partial_diagnostics() {
        let rendered = render_find_estimate_text(&json!({"estimate": {}}), None);
        assert!(rendered.contains("- scope: all tracked paths"));
        assert!(rendered.contains("- peak memory: ~0 MiB (0-0 MiB; 0 MiB budget)"));
        assert!(rendered.contains("- confidence: unknown"));
    }
}
