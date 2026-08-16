#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn size_parser_and_percentile_are_stable() {
        assert_eq!(parse_size("12.5M"), Some(13_107_200));
        assert_eq!(parse_size("2G"), Some(2_147_483_648));
        assert_eq!(p95(vec![1, 2, 3, 4, 5]), Some(5));
    }

    #[test]
    fn only_runtime_provider_failures_trigger_the_bounded_abort() {
        assert!(is_provider_runtime_failure(Some("provider_http_error")));
        assert!(is_provider_runtime_failure(Some("provider_timeout")));
        assert!(!is_provider_runtime_failure(Some("provider_response_invalid")));
        assert!(!is_provider_runtime_failure(Some("artifact_invalid")));
        assert!(!is_provider_runtime_failure(None));
        assert_eq!(BENCHMARK_CONSECUTIVE_PROVIDER_FAILURE_LIMIT, 2);
    }

    #[test]
    fn semantic_report_fingerprint_ignores_only_runtime_measurements() {
        let left = serde_json::to_vec(&json!({
            "repo": {"head_sha": "a"},
            "diagnostics": {
                "analysis": {
                    "analysis_elapsed_ms_before_report": 10,
                    "estimator_error_ratio": 0.1,
                    "measured_peak_rss_bytes": 100
                },
                "report_sizes": {"logical_artifact_bytes": 200, "report_json_bytes": 200}
            }
        }))
        .unwrap();
        let right = serde_json::to_vec(&json!({
            "repo": {"head_sha": "a"},
            "diagnostics": {
                "analysis": {
                    "analysis_elapsed_ms_before_report": 20,
                    "estimator_error_ratio": 0.2,
                    "measured_peak_rss_bytes": 300
                },
                "report_sizes": {"logical_artifact_bytes": 400, "report_json_bytes": 400}
            }
        }))
        .unwrap();
        assert_eq!(
            semantic_report_sha256(&left).unwrap(),
            semantic_report_sha256(&right).unwrap()
        );
        let changed = String::from_utf8(right)
            .unwrap()
            .replace("\"head_sha\":\"a\"", "\"head_sha\":\"b\"");
        assert_ne!(
            semantic_report_sha256(&left).unwrap(),
            semantic_report_sha256(changed.as_bytes()).unwrap()
        );
    }

    #[test]
    fn committed_corpus_is_privacy_safe_and_valid() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
        let bytes = fs::read(root.join("benchmarks/advisor/corpus-v1.json")).unwrap();
        let corpus: Corpus = serde_json::from_slice(&bytes).unwrap();
        validate_corpus(&corpus).unwrap();
        let text = String::from_utf8(bytes).unwrap();
        for forbidden in ["/Users/", "<|", "reasoning_content", "source_excerpt"] {
            assert!(!text.contains(forbidden));
        }
    }

    #[test]
    fn release_recommendations_require_the_full_repeated_matrix() {
        let mut options = Options {
            repo_root: PathBuf::new(),
            binary: PathBuf::new(),
            corpus: PathBuf::new(),
            thresholds: PathBuf::new(),
            repositories: Vec::new(),
            provider: "ollama".to_string(),
            endpoint: "http://127.0.0.1:11434/api/chat".to_string(),
            model: "openai/gpt-oss-safeguard-20b".to_string(),
            runtime_model: "gpt-oss-safeguard:20b".to_string(),
            runtime_label: "test".to_string(),
            model_digest: "not-applicable".to_string(),
            model_quantization: "not-applicable".to_string(),
            model_size_bytes: None,
            estimated_peak_memory_bytes: None,
            confirm_dedicated_host: false,
            initial_runtime_state: "not-applicable".to_string(),
            output_dir: PathBuf::new(),
            repetitions: 3,
            full_matrix: false,
            prepare_only: true,
            ratings: None,
            review_output_dir: None,
        };
        assert!(!release_matrix_complete(&options));
        options.full_matrix = true;
        options.repetitions = 2;
        assert!(!release_matrix_complete(&options));
        options.repetitions = 3;
        assert!(release_matrix_complete(&options));
    }

    #[test]
    fn dedicated_benchmark_rejects_the_recorded_sixteen_gib_capacity() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
        let gate: BenchmarkReleaseGate = serde_json::from_slice(
            &fs::read(root.join("benchmarks/advisor/release-gate.json")).unwrap(),
        )
        .unwrap();
        let error = validate_benchmark_capacity(
            &gate,
            13_793_441_254,
            17_179_869_184,
            16 * 1024 * 1024 * 1024,
            15 * 1024 * 1024 * 1024,
            0,
        )
        .expect_err("16 GiB benchmark host must fail");
        assert!(error.to_string().contains("do not run on this host"));
    }

    #[test]
    fn completed_results_can_be_finalized_without_rerunning_inference() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
        let corpus_path = root.join("benchmarks/advisor/corpus-v1.json");
        let thresholds_path = root.join("benchmarks/advisor/thresholds-v1.json");
        let corpus_bytes = fs::read(&corpus_path).unwrap();
        let thresholds_bytes = fs::read(&thresholds_path).unwrap();
        let corpus: Corpus = serde_json::from_slice(&corpus_bytes).unwrap();
        let temporary = tempfile::tempdir().unwrap();
        let results = temporary.path().join("results.json");
        fs::write(
            &results,
            serde_json::to_vec_pretty(&json!({
                "schema_version": 1,
                "status": "complete",
                "configuration": {
                    "corpus_sha256": sha256(&corpus_bytes),
                    "thresholds_sha256": sha256(&thresholds_bytes)
                },
                "system": {},
                "summary": {"automatic_gates_passed": true},
                "recommended_configuration": {"reasoning_effort": "medium"},
                "recommendation": "adjust"
            }))
            .unwrap(),
        )
        .unwrap();
        fs::write(
            temporary.path().join("decision.md"),
            "# Decision\n\n- Recommendation: **adjust**\n- Maintainer usefulness mean: not reviewed\n- Manual quality mean: not reviewed\n- Unsupported claims found by maintainers: not reviewed\n",
        )
        .unwrap();
        let ratings_path = temporary.path().join("ratings.json");
        let cases = corpus
            .cases
            .iter()
            .map(|case| {
                (
                    case.id.clone(),
                    json!({
                        "recommendation_usefulness": 5,
                        "fact_interpretation_separation": 5,
                        "scope_quality": 5,
                        "verification_quality": 5,
                        "actionability": 5,
                        "unsupported_claim_count": 0
                    }),
                )
            })
            .collect::<BTreeMap<_, _>>();
        fs::write(
            &ratings_path,
            serde_json::to_vec_pretty(&json!({
                "schema_version": 1,
                "reviewer_count": 1,
                "cases": cases
            }))
            .unwrap(),
        )
        .unwrap();

        finalize(
            root,
            &corpus_path,
            &thresholds_path,
            &results,
            &ratings_path,
        )
        .unwrap();
        let finalized: Value = serde_json::from_slice(&fs::read(&results).unwrap()).unwrap();
        assert_eq!(finalized["recommendation"], "ship");
        assert_eq!(
            finalized["summary"]["manual_quality_gates_passed"],
            true
        );
        assert_eq!(
            finalized["manual_ratings_sha256"].as_str().map(str::len),
            Some(64)
        );
    }
}
