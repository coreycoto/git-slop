fn write_preflight(
    options: &Options,
    corpus: &Corpus,
    reports: &BTreeMap<String, PreparedReport>,
    provenance: &BenchmarkProvenance,
) -> Result<(PathBuf, PathBuf)> {
    let output_dir = resolve(&options.repo_root, &options.output_dir);
    let repositories = reports
        .iter()
        .map(|(key, report)| {
            let fixture = corpus
                .repositories
                .get(key)
                .expect("prepared report has a corpus fixture");
            (
                key,
                json!({
                    "revision": fixture.revision,
                    "as_of": fixture.as_of,
                    "report_sha256": report.sha256,
                    "matches_expected": fixture.expected_report_sha256.as_deref() == Some(report.sha256.as_str())
                }),
            )
        })
        .collect::<BTreeMap<_, _>>();
    let result = json!({
        "schema_version": 1,
        "status": "incomplete",
        "configuration": {
            "corpus": corpus.id,
            "mode": "prepare-only",
            "corpus_sha256": sha256(&read_bounded(&resolve(&options.repo_root, &options.corpus), MAX_BENCHMARK_CONFIG_BYTES, "advisor corpus")?),
            "thresholds_sha256": sha256(&read_bounded(&resolve(&options.repo_root, &options.thresholds), MAX_BENCHMARK_CONFIG_BYTES, "advisor thresholds")?),
            "harness_revision": provenance.harness_revision,
            "binary_sha256": provenance.binary_sha256,
            "binary_source_revision": provenance.binary_source_revision,
            "binary_version": provenance.binary_version,
            "binary_target": provenance.binary_target,
            "binary_build_source": provenance.binary_build_source,
            "binary_inference_feature_enabled": provenance.binary_inference_feature_enabled
        },
        "system": system_profile(),
        "repositories": repositories,
        "recommended_configuration": Value::Null,
        "recommendation": "defer",
        "next_step": "Review the deterministic candidates, then pin these report fingerprints before the live model matrix."
    });
    let json_path = output_dir.join("results.json");
    let markdown_path = output_dir.join("decision.md");
    let markdown = "# Safeguard-only V1 decision\n\n- Recommendation: **defer**\n- Status: deterministic report preparation only; no model inference was attempted.\n\nReview and pin the privacy-safe report fingerprints in `results.json` before the live matrix.\n";
    write_benchmark_pair(&json_path, &result, &markdown_path, markdown)?;
    Ok((json_path, markdown_path))
}
