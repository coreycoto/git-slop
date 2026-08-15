fn print_scan_receipt(result: &crate::FindResult) {
    let diagnostics = result
        .report
        .pointer("/diagnostics/analysis")
        .unwrap_or(&Value::Null);
    let stats = result.report.get("stats").unwrap_or(&Value::Null);
    let skipped_count = |key: &str| stats.get(key).and_then(Value::as_u64).unwrap_or_default();
    let skipped_ignored = skipped_count("skipped_ignored_count");
    let skipped_missing = skipped_count("skipped_missing_count");
    let skipped_binary = skipped_count("skipped_binary_count");
    let skipped_undecodable = skipped_count("skipped_undecodable_count");
    let skipped = skipped_ignored
        .saturating_add(skipped_missing)
        .saturating_add(skipped_binary)
        .saturating_add(skipped_undecodable);
    let output_root = result
        .report_json
        .parent()
        .and_then(Path::parent)
        .unwrap_or_else(|| result.report_json.parent().unwrap_or(Path::new(".")));
    println!(
        "Scan receipt: {:.2}s; paths: tracked={}, analyzed={}, skipped={} (ignored={}, missing={}, binary={}, undecodable={}); commits={} examined; cache={} hit(s)/{} miss(es); peak={} MiB; report={} KiB; profile={}; output={}.",
        result.elapsed_ms as f64 / 1_000.0,
        stats
            .get("tracked_file_count")
            .and_then(Value::as_u64)
            .unwrap_or_default(),
        stats
            .get("analyzed_file_count")
            .and_then(Value::as_u64)
            .unwrap_or_default(),
        skipped,
        skipped_ignored,
        skipped_missing,
        skipped_binary,
        skipped_undecodable,
        diagnostics
            .pointer("/history/window_numstat_commit_count")
            .and_then(Value::as_u64)
            .unwrap_or_default(),
        diagnostics
            .get("cache_hits")
            .and_then(Value::as_u64)
            .unwrap_or_default(),
        diagnostics
            .get("cache_misses")
            .and_then(Value::as_u64)
            .unwrap_or_default(),
        diagnostics
            .get("measured_peak_rss_bytes")
            .and_then(Value::as_u64)
            .unwrap_or_default()
            .div_ceil(1024 * 1024),
        result
            .report
            .pointer("/diagnostics/report_sizes/report_json_bytes")
            .and_then(Value::as_u64)
            .unwrap_or_default()
            .div_ceil(1024),
        result
            .report
            .pointer("/analyzer/report_profile")
            .and_then(Value::as_str)
            .unwrap_or("standard"),
        output_root.display(),
    );
}
