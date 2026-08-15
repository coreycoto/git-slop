fn print_find_estimate(
    payload: &Value,
    normalized_scope: Option<&str>,
    format: DisplayFormat,
) -> Result<()> {
    match format {
        DisplayFormat::Json => print_text(&render_json(payload)?),
        DisplayFormat::Yaml => print_text(&serde_yaml::to_string(payload)?),
        DisplayFormat::Text => {
            let estimate = &payload["estimate"];
            let mib = |key: &str| {
                estimate[key]
                    .as_u64()
                    .unwrap_or_default()
                    .div_ceil(1024 * 1024)
            };
            println!("Git Slop scan estimate");
            println!(
                "- scope: {}",
                normalized_scope.unwrap_or("all tracked paths")
            );
            println!("- tracked paths: {}", estimate["tracked_path_count"]);
            println!(
                "- peak memory: ~{} MiB ({}-{} MiB; {} MiB budget)",
                mib("estimated_peak_memory_bytes"),
                mib("estimated_peak_memory_low_bytes"),
                mib("estimated_peak_memory_high_bytes"),
                mib("memory_budget_bytes")
            );
            println!(
                "- cache/report: ~{} MiB / ~{} MiB",
                mib("estimated_cache_bytes"),
                mib("estimated_report_bytes")
            );
            println!(
                "- time: ~{}s cold / ~{}s warm",
                estimate["estimated_seconds_cold"], estimate["estimated_seconds_warm"]
            );
            println!("- inodes: ~{}", estimate["estimated_inode_count"]);
            println!(
                "- cache assumptions: {}",
                estimate["cache_assumptions"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str)
                    .collect::<Vec<_>>()
                    .join("; ")
            );
            println!(
                "- confidence: {}",
                estimate["confidence"].as_str().unwrap_or("unknown")
            );
            println!("Next: run `git slop find` when the estimate fits your budget.");
        }
    }
    Ok(())
}
