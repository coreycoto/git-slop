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
    fn benchmark_runtime_identity_cannot_record_a_local_path() {
        assert!(privacy_safe_benchmark_runtime_identifier(
            "org/model@sha256:abc"
        ));
        assert!(!privacy_safe_benchmark_runtime_identifier(
            "/Users/example/model"
        ));
        assert!(!privacy_safe_benchmark_runtime_identifier(
            "C:/Users/example/model"
        ));
    }

    #[test]
    fn paired_benchmark_rollback_restores_both_originals() {
        let temporary = tempfile::tempdir().unwrap();
        let results = temporary.path().join("results.json");
        let decision = temporary.path().join("decision.md");
        let results_backup = temporary.path().join("results.backup");
        let decision_backup = temporary.path().join("decision.backup");
        fs::write(&results, b"new results").unwrap();
        fs::write(&decision, b"new decision").unwrap();
        fs::write(&results_backup, b"old results").unwrap();
        fs::write(&decision_backup, b"old decision").unwrap();

        restore_pair(
            &results,
            &results_backup,
            true,
            &decision,
            &decision_backup,
            true,
        )
        .unwrap();

        assert_eq!(fs::read(results).unwrap(), b"old results");
        assert_eq!(fs::read(decision).unwrap(), b"old decision");
    }

    #[test]
    fn only_runtime_provider_failures_trigger_the_bounded_abort() {
        assert!(is_provider_runtime_failure(Some("provider_http_error")));
        assert!(is_provider_runtime_failure(Some("provider_http_invalid")));
        assert!(is_provider_runtime_failure(Some("provider_http_unsupported")));
        assert!(is_provider_runtime_failure(Some("provider_timeout")));
        assert!(!is_provider_runtime_failure(Some("provider_response_invalid")));
        assert!(!is_provider_runtime_failure(Some("artifact_invalid")));
        assert!(!is_provider_runtime_failure(None));
        assert_eq!(BENCHMARK_CONSECUTIVE_PROVIDER_FAILURE_LIMIT, 2);
        assert!(is_terminal_provider_identity_failure(Some(
            "provider_model_mismatch"
        )));
        assert!(is_terminal_provider_identity_failure(Some(
            "provider_model_identity_missing"
        )));
        assert!(!is_terminal_provider_identity_failure(Some(
            "provider_incomplete_response"
        )));
        assert_eq!(
            classify_failure(
                br#"{"error":{"code":"provider_model_mismatch","message":"mismatch"}}"#,
                false,
            ),
            "provider_model_mismatch"
        );
        assert_eq!(
            classify_failure(b"error: provider_model_mismatch", false),
            "artifact_unavailable"
        );
    }

    #[test]
    fn benchmark_independently_rejects_an_invented_artifact_reference() {
        let candidate_id = "candidate-0123456789abcdef";
        let empty_citations = || {
            json!({
                "candidates": [], "paths": [], "findings": [],
                "relationships": [], "clusters": [], "excerpts": [],
                "policies": [], "verification": []
            })
        };
        let mut artifact = json!({
            "schema_version": 1,
            "command": "advise",
            "generated_at": "2026-08-17T00:00:00Z",
            "report": {"sha256": "a".repeat(64), "canonical_sha256": "b".repeat(64)},
            "selector": {},
            "candidate_ids": [candidate_id],
            "context": {
                "builder_version": 1,
                "digest": "c".repeat(64),
                "limits": {},
                "missing_evidence": [],
                "reference_index": {
                    "candidates": [candidate_id], "paths": [], "findings": [],
                    "relationships": [], "clusters": [], "excerpts": [],
                    "policies": [], "verification": []
                }
            },
            "policies": {"resolution_digest": "d".repeat(64), "packs": [], "conflicts": []},
            "provider": {
                "provider": "mock", "model": "test", "requested_runtime_model": "test",
                "endpoint_classification": "none", "reasoning_effort": "medium",
                "timeout_ms": 1, "max_response_bytes": 1, "max_output_tokens": 1,
                "context_window_tokens": 1, "runtime_label": null, "model_digest": null
            },
            "timing": {
                "context_elapsed_ms": 0, "provider_elapsed_ms": 0,
                "validation_elapsed_ms": 0, "time_to_validated_artifact_ms": 0
            },
            "response_sha256": "e".repeat(64),
            "evaluation": {
                "schema_version": 1,
                "reported_aggregate_verdict": "approve",
                "aggregate_verdict": "approve",
                "summary": "test",
                "candidate_evaluations": [{
                    "candidate_id": candidate_id,
                    "aggregate_verdict": "approve",
                    "rationale": "test",
                    "citations": empty_citations(),
                    "rule_evaluations": [{
                        "rule_id": "test-rule", "verdict": "approve",
                        "rationale": "test", "citations": empty_citations()
                    }],
                    "requested_revisions": [], "verification_steps": []
                }],
                "warnings": []
            },
            "validation": {
                "status": "valid", "aggregate_recomputed": true,
                "references_validated": true, "warnings": []
            },
            "boundary": "Policy-guided advice is non-mutating and advisory. It cannot rewrite detector truth or change git slop check results."
        });
        let report = PreparedReport {
            path: PathBuf::new(),
            sha256: "f".repeat(64),
            raw_sha256: "a".repeat(64),
            canonical_sha256: "b".repeat(64),
        };
        let assessment = assess_advice_artifact(&artifact, &report, 1).unwrap();
        assert!(assessment.valid);
        artifact["evaluation"]["candidate_evaluations"][0]["citations"]["paths"] =
            json!(["src/invented.rs"]);
        let assessment = assess_advice_artifact(&artifact, &report, 1).unwrap();
        assert!(!assessment.valid);
        assert_eq!(assessment.invalid_references, 1);
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
    fn dedicated_benchmark_rejects_existing_swap_pressure() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
        let gate: BenchmarkReleaseGate = serde_json::from_slice(
            &fs::read(root.join("benchmarks/advisor/release-gate.json")).unwrap(),
        )
        .unwrap();
        let error = validate_benchmark_capacity(
            &gate,
            13_793_441_254,
            17_179_869_184,
            64 * 1024 * 1024 * 1024,
            48 * 1024 * 1024 * 1024,
            512 * 1024 * 1024,
        )
        .expect_err("initial swap pressure must fail");
        assert!(error.to_string().contains("swap in use"));
    }

    #[test]
    fn capacity_receipt_reports_every_host_blocker() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
        let gate: BenchmarkReleaseGate = serde_json::from_slice(
            &fs::read(root.join("benchmarks/advisor/release-gate.json")).unwrap(),
        )
        .unwrap();
        let (_, _, blockers) = benchmark_capacity_blockers(
            &gate,
            13_793_441_254,
            17_179_869_184,
            16 * 1024 * 1024 * 1024,
            4 * 1024 * 1024 * 1024,
            512 * 1024 * 1024,
        )
        .expect("capacity evaluation");
        let codes = blockers
            .iter()
            .map(|blocker| blocker.code)
            .collect::<Vec<_>>();
        assert_eq!(
            codes,
            [
                "physical_memory_below_required",
                "available_memory_below_required",
                "initial_swap_above_maximum"
            ]
        );
    }

    #[test]
    fn benchmark_child_output_is_drained_but_retained_within_a_fixed_limit() {
        let exceeded = Arc::new(AtomicBool::new(false));
        let result = drain_bounded(
            std::io::Cursor::new(b"abcdefgh"),
            4,
            Arc::clone(&exceeded),
        )
        .expect("bounded drain");
        assert_eq!(result.bytes, b"abcd");
        assert!(result.truncated);
        assert!(exceeded.load(Ordering::Acquire));

        let exceeded = Arc::new(AtomicBool::new(false));
        let result = drain_bounded(
            std::io::Cursor::new(b"abcd"),
            4,
            Arc::clone(&exceeded),
        )
        .expect("exact bounded drain");
        assert_eq!(result.bytes, b"abcd");
        assert!(!result.truncated);
        assert!(!exceeded.load(Ordering::Acquire));
    }

    #[test]
    fn benchmark_gate_cannot_weaken_the_fixed_runtime_floor() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
        let mut gate: BenchmarkReleaseGate = serde_json::from_slice(
            &fs::read(root.join("benchmarks/advisor/release-gate.json")).unwrap(),
        )
        .unwrap();
        gate.minimum_available_memory_reserve_bytes = 4 * 1024 * 1024 * 1024;
        assert!(validate_benchmark_gate(&gate).is_err());
    }

    #[test]
    fn completed_results_can_be_finalized_without_rerunning_inference() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
        let corpus_path = root.join("benchmarks/advisor/corpus-v1.json");
        let thresholds_path = root.join("benchmarks/advisor/thresholds-v1.json");
        let corpus_bytes = fs::read(&corpus_path).unwrap();
        let thresholds_bytes = fs::read(&thresholds_path).unwrap();
        let corpus: Corpus = serde_json::from_slice(&corpus_bytes).unwrap();
        let thresholds: Thresholds = serde_json::from_slice(&thresholds_bytes).unwrap();
        let temporary = tempfile::tempdir().unwrap();
        let reports = corpus
            .repositories
            .iter()
            .map(|(key, repository)| {
                (
                    key.clone(),
                    PreparedReport {
                        path: temporary.path().join(format!("{key}-report.json")),
                        sha256: repository
                            .expected_report_sha256
                            .clone()
                            .unwrap_or_else(|| "0".repeat(64)),
                        raw_sha256: "3".repeat(64),
                        canonical_sha256: "4".repeat(64),
                    },
                )
            })
            .collect::<BTreeMap<_, _>>();
        let options = Options {
            repo_root: root.to_path_buf(),
            binary: PathBuf::new(),
            corpus: corpus_path.clone(),
            thresholds: thresholds_path.clone(),
            repositories: Vec::new(),
            provider: "openai-compatible".to_string(),
            endpoint: "http://127.0.0.1:12345/v1/chat/completions".to_string(),
            model: "openai/gpt-oss-safeguard-20b".to_string(),
            runtime_model: "synthetic-runtime-model".to_string(),
            runtime_label: "synthetic-runtime".to_string(),
            model_digest: format!("sha256:{}", "a".repeat(64)),
            model_quantization: "Q4_K_M".to_string(),
            model_size_bytes: Some(13_793_441_254),
            estimated_peak_memory_bytes: Some(17_179_869_184),
            confirm_dedicated_host: true,
            initial_runtime_state: "cold".to_string(),
            output_dir: temporary.path().to_path_buf(),
            repetitions: 1,
            full_matrix: false,
            prepare_only: false,
            review_output_dir: None,
        };
        let samples = corpus
            .cases
            .iter()
            .enumerate()
            .map(|(index, case)| Sample {
                case_id: case.id.clone(),
                repository: case.repository.clone(),
                scenario_tags: case.scenario_tags.clone(),
                scenario: case.scenario.clone(),
                candidate_count: case.candidate_count,
                actual_candidate_count: None,
                report_sha256: reports[&case.repository].sha256.clone(),
                reasoning_effort: "medium".to_string(),
                context_token_limit: 8_192,
                output_token_limit: case.candidate_count.saturating_mul(2_048).min(8_192),
                repetition: 1,
                phase: if index == 0 { "cold" } else { "warm" }.to_string(),
                status: "failed".to_string(),
                exit_code: Some(2),
                total_elapsed_ms: 1,
                peak_process_rss_bytes: None,
                system_available_memory_before_bytes: None,
                system_available_memory_after_bytes: None,
                system_available_memory_minimum_bytes: None,
                swap_before_bytes: None,
                swap_after_bytes: None,
                swap_growth_bytes: None,
                context_elapsed_ms: None,
                provider_elapsed_ms: None,
                validation_elapsed_ms: None,
                time_to_validated_artifact_ms: None,
                model_load_duration_ns: None,
                prompt_eval_duration_ns: None,
                generation_duration_ns: None,
                input_tokens: None,
                output_tokens: None,
                prompt_tokens_per_second: None,
                output_tokens_per_second: None,
                reported_aggregate: None,
                expected_aggregate: case.expected_aggregate.clone(),
                aggregate_match: false,
                matched_rule_verdicts: 0,
                expected_rule_verdicts: high_severity_expectation_count(
                    &case.expected_rule_verdicts,
                ) * case.candidate_count,
                accepted_invalid_references: 0,
                accepted_detector_truth_changes: 0,
                citation_complete: false,
                retry_count: 0,
                failure_category: Some("provider_response_invalid".to_string()),
            })
            .collect::<Vec<_>>();
        let (results, _) = write_outputs(
            &options,
            &OutputInputs {
                corpus: &corpus,
                reports: &reports,
                thresholds: &thresholds,
                provenance: &test_provenance(),
            },
            1,
            &samples,
            None,
            None,
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

        let decision = temporary.path().join("decision.md");
        let original_decision = fs::read(&decision).unwrap();
        let original_results = fs::read(&results).unwrap();
        let mut tampered: Value = serde_json::from_slice(&original_results).unwrap();
        tampered["summary"]["automatic_gates_passed"] = json!(true);
        fs::write(
            &results,
            serde_json::to_string_pretty(&tampered).unwrap() + "\n",
        )
        .unwrap();
        let tampered_bytes = fs::read(&results).unwrap();
        let error = finalize(
            root,
            &corpus_path,
            &thresholds_path,
            &results,
            &ratings_path,
        )
        .expect_err("derived gate drift must fail before mutation");
        assert!(error.to_string().contains("automatic_gates_passed"));
        assert_eq!(fs::read(&results).unwrap(), tampered_bytes);
        fs::write(&results, &original_results).unwrap();

        let mut truncated: Value = serde_json::from_slice(&original_results).unwrap();
        truncated["samples"]
            .as_array_mut()
            .expect("benchmark samples")
            .pop();
        fs::write(
            &results,
            serde_json::to_string_pretty(&truncated).unwrap() + "\n",
        )
        .unwrap();
        let truncated_bytes = fs::read(&results).unwrap();
        let error = finalize(
            root,
            &corpus_path,
            &thresholds_path,
            &results,
            &ratings_path,
        )
        .expect_err("truncated sample evidence must fail before mutation");
        assert!(error.to_string().contains("sample matrix ended"));
        assert_eq!(fs::read(&results).unwrap(), truncated_bytes);
        fs::write(&results, &original_results).unwrap();

        fs::write(&decision, "# Incomplete decision template\n").unwrap();
        let error = finalize(
            root,
            &corpus_path,
            &thresholds_path,
            &results,
            &ratings_path,
        )
        .expect_err("invalid decision template must fail before mutation");
        assert!(
            error
                .to_string()
                .contains("does not match its result evidence")
        );
        assert_eq!(fs::read(&results).unwrap(), original_results);
        fs::write(&decision, original_decision).unwrap();

        finalize(
            root,
            &corpus_path,
            &thresholds_path,
            &results,
            &ratings_path,
        )
        .unwrap();
        let finalized: Value = serde_json::from_slice(&fs::read(&results).unwrap()).unwrap();
        assert_eq!(finalized["recommendation"], "defer");
        assert_eq!(
            finalized["summary"]["manual_quality_gates_passed"],
            true
        );
        assert_eq!(
            finalized["manual_ratings_sha256"].as_str().map(str::len),
            Some(64)
        );
        let finalized_bytes = fs::read(&results).unwrap();
        let error = finalize(
            root,
            &corpus_path,
            &thresholds_path,
            &results,
            &ratings_path,
        )
        .expect_err("finalized results must not be overwritten");
        assert!(error.to_string().contains("already finalized"));
        assert_eq!(fs::read(&results).unwrap(), finalized_bytes);
    }
}
