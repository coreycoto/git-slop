use super::*;

pub(super) struct OutputInputs<'a> {
    pub(super) corpus: &'a Corpus,
    pub(super) reports: &'a BTreeMap<String, PreparedReport>,
    pub(super) thresholds: &'a Thresholds,
    pub(super) provenance: &'a BenchmarkProvenance,
}

pub(super) fn write_outputs(
    options: &Options,
    inputs: &OutputInputs<'_>,
    started: u128,
    samples: &[Sample],
    manual: Option<&ManualScores>,
    termination_reason: Option<&str>,
) -> Result<(PathBuf, PathBuf)> {
    let OutputInputs {
        corpus,
        reports,
        thresholds,
        provenance,
    } = inputs;
    let report_digests = reports
        .iter()
        .map(|(key, report)| (key.clone(), report.sha256.clone()))
        .collect::<BTreeMap<_, _>>();
    verify_sample_matrix(
        options,
        corpus,
        &report_digests,
        samples,
        termination_reason.is_none(),
    )?;
    let corpus_pinned = corpus.repositories.iter().all(|(key, repository)| {
        repository.expected_report_sha256.as_deref()
            == reports.get(key).map(|report| report.sha256.as_str())
    });
    let derivation = derive_benchmark(
        options,
        thresholds,
        samples,
        corpus_pinned,
        manual,
        termination_reason,
    )?;
    let repositories = corpus
        .repositories
        .iter()
        .map(|(key, fixture)| {
            let report_sha256 = reports.get(key).map(|report| report.sha256.as_str());
            (
                key,
                json!({
                    "revision": fixture.revision,
                    "as_of": fixture.as_of,
                    "report_sha256": report_sha256,
                    "matches_expected": fixture.expected_report_sha256.as_deref() == report_sha256
                }),
            )
        })
        .collect::<BTreeMap<_, _>>();
    let mut result = json!({
        "schema_version": 1,
        "status": derivation.status,
        "started_unix_ms": started,
        "finished_unix_ms": now_ms(),
        "configuration": {
            "corpus": "corpus-v1", "thresholds": "thresholds-v1",
            "corpus_sha256": sha256(&read_bounded(&resolve(&options.repo_root, &options.corpus), MAX_BENCHMARK_CONFIG_BYTES, "advisor corpus")?),
            "thresholds_sha256": sha256(&read_bounded(&resolve(&options.repo_root, &options.thresholds), MAX_BENCHMARK_CONFIG_BYTES, "advisor thresholds")?),
            "provider": options.provider, "runtime_label": options.runtime_label, "runtime_model": options.runtime_model,
            "model": options.model, "model_digest": options.model_digest,
            "model_quantization": options.model_quantization,
            "model_size_bytes": options.model_size_bytes,
            "estimated_peak_memory_bytes": options.estimated_peak_memory_bytes,
            "dedicated_host_confirmed": options.confirm_dedicated_host,
            "initial_runtime_state": options.initial_runtime_state,
            "runtime_context_tokens": BENCHMARK_RUNTIME_CONTEXT_TOKENS,
            "request_timeout_seconds": BENCHMARK_TIMEOUT_SECONDS,
            "child_output_limit_bytes": BENCHMARK_CHILD_OUTPUT_LIMIT_BYTES,
            "endpoint_classification": "loopback",
            "repetitions": options.repetitions, "full_matrix": options.full_matrix,
            "repository_keys": reports.keys().map(String::as_str).collect::<BTreeSet<_>>(),
            "repository_revisions": corpus.repositories.iter().map(|(key, fixture)| (key, fixture.revision.as_str())).collect::<BTreeMap<_, _>>(),
            "harness_revision": provenance.harness_revision,
            "binary_sha256": provenance.binary_sha256,
            "binary_source_revision": provenance.binary_source_revision,
            "binary_version": provenance.binary_version,
            "binary_target": provenance.binary_target,
            "binary_build_source": provenance.binary_build_source,
            "binary_inference_feature_enabled": provenance.binary_inference_feature_enabled,
            "child_deadline_seconds": BENCHMARK_CHILD_DEADLINE_SECONDS
        },
        "system": system_profile(),
        "thresholds": thresholds,
        "summary": derivation.summary,
        "samples": samples,
        "repositories": repositories,
        "recommended_configuration": derivation.recommended_configuration,
        "recommendation": derivation.recommendation
    });
    if derivation.status == BenchmarkStatus::Incomplete {
        let next_step = match derivation
            .termination
            .expect("incomplete derivation has a termination state")
        {
            BenchmarkTermination::ProviderModelIdentityMissing
            | BenchmarkTermination::ProviderModelMismatch => {
                "Do not retry or accept evidence from this runtime. Verify the separately provisioned served-model identity before authorizing a fresh run."
            }
            BenchmarkTermination::BenchmarkChildOutputLimit => {
                "Do not retry until the unexpected child output volume is understood and bounded. Inspect the retained diagnostics without starting a provider on this host."
            }
            BenchmarkTermination::BenchmarkCheckpoint => {
                "The benchmark is still running. This fail-closed checkpoint preserves completed cells and cannot authorize release."
            }
            BenchmarkTermination::OperatorInterrupt => {
                "The operator interrupted the benchmark. Inspect the preserved incomplete evidence before authorizing any fresh run."
            }
            BenchmarkTermination::BenchmarkChildDeadline => {
                "The parent-owned child deadline expired. Inspect the preserved incomplete evidence; do not retry until the stall is understood."
            }
            BenchmarkTermination::ResourceGuardAvailableMemory
            | BenchmarkTermination::ResourceGuardMeasurementUnavailable
            | BenchmarkTermination::ResourceGuardSwapGrowth
            | BenchmarkTermination::ConsecutiveProviderRuntimeFailures => {
                "Do not retry on this host. Inspect the safety-guard result, recover the runtime separately, and use a different adequately resourced dedicated host."
            }
        };
        result
            .as_object_mut()
            .expect("benchmark result is an object")
            .insert("next_step".to_string(), json!(next_step));
    }
    let output_dir = resolve(&options.repo_root, &options.output_dir);
    let json_path = output_dir.join("results.json");
    let markdown_path = output_dir.join("decision.md");
    let markdown = render_live_decision(&result)?;
    write_benchmark_pair(&json_path, &result, &markdown_path, &markdown, true)?;
    Ok((json_path, markdown_path))
}
