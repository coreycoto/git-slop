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
    fn interrupted_benchmark_pair_transaction_recovers_both_originals() {
        let temporary = tempfile::tempdir().unwrap();
        let results = temporary.path().join("results.json");
        let decision = temporary.path().join("decision.md");
        let results_backup = temporary.path().join("results.backup");
        let decision_backup = temporary.path().join("decision.backup");
        let decision_temporary = temporary.path().join("decision.tmp");
        fs::write(&results, b"new partial results").unwrap();
        fs::write(&results_backup, b"old results").unwrap();
        fs::write(&decision_backup, b"old decision").unwrap();
        fs::write(&decision_temporary, b"new decision").unwrap();
        fs::write(
            temporary.path().join(".benchmark-pair.transaction.json"),
            serde_json::to_vec(&BenchmarkPairTransaction {
                schema_version: 1,
                results_file: "results.json".to_string(),
                decision_file: "decision.md".to_string(),
                results_temporary: "results.tmp".to_string(),
                decision_temporary: "decision.tmp".to_string(),
                results_backup: "results.backup".to_string(),
                decision_backup: "decision.backup".to_string(),
                had_results: true,
                had_decision: true,
            })
            .unwrap(),
        )
        .unwrap();

        assert!(
            recover_benchmark_pair(temporary.path(), &results, &decision).unwrap()
        );
        assert_eq!(fs::read(results).unwrap(), b"old results");
        assert_eq!(fs::read(decision).unwrap(), b"old decision");
        assert!(!decision_temporary.exists());
        assert!(
            !temporary
                .path()
                .join(".benchmark-pair.transaction.json")
                .exists()
        );
    }

    #[test]
    fn threshold_schema_and_runtime_deserialization_fail_together() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
        let standalone_schema: Value = serde_json::from_slice(
            &fs::read(root.join("schemas/advisor-thresholds-1.json")).unwrap(),
        )
        .unwrap();
        let benchmark_schema: Value = serde_json::from_slice(
            &fs::read(root.join("schemas/advisor-benchmark-1.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(
            standalone_schema["required"],
            benchmark_schema["$defs"]["thresholds"]["required"]
        );
        assert_eq!(
            standalone_schema["properties"],
            benchmark_schema["$defs"]["thresholds"]["properties"]
        );
        let bytes = fs::read(root.join("benchmarks/advisor/thresholds-v1.json")).unwrap();
        let thresholds = parse_thresholds(&bytes).expect("checked-in thresholds");
        assert_eq!(thresholds.accepted_invalid_reference_maximum, 0);
        let mut invalid: Value = serde_json::from_slice(&bytes).unwrap();
        invalid["structured_output_success_rate_minimum"] = json!(1.1);
        assert!(parse_thresholds(&serde_json::to_vec(&invalid).unwrap()).is_err());
        invalid["structured_output_success_rate_minimum"] = json!(0.95);
        invalid["unexpected"] = json!(true);
        assert!(parse_thresholds(&serde_json::to_vec(&invalid).unwrap()).is_err());
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
        let citations = |policy: bool| {
            json!({
                "candidates": [candidate_id], "paths": [], "findings": [],
                "relationships": [], "clusters": [], "excerpts": [],
                "policies": if policy { vec!["test-rule"] } else { Vec::<&str>::new() },
                "verification": []
            })
        };
        let mut artifact = json!({
            "schema_version": 1,
            "command": "advise",
            "generated_at": "2026-08-17T00:00:00Z",
            "report": {
                "schema_version": 5, "sha256": "a".repeat(64),
                "canonical_sha256": "b".repeat(64), "repository_id": "fixture",
                "head_sha": "1".repeat(40), "worktree_clean": true,
                "worktree_state_digest": "2".repeat(64),
                "scope": {"mode": "repository", "path": null, "selected_path_count": 1, "selected_path_digest": "3".repeat(64)}
            },
            "selector": {"kind": "top", "value": 1},
            "candidate_ids": [candidate_id],
            "context": {
                "builder_version": 2,
                "digest": "c".repeat(64),
                "limits": {
                    "maximum_context_bytes": 131072, "maximum_context_tokens": 8192,
                    "estimated_context_tokens": 100, "per_excerpt_bytes": 4096,
                    "maximum_files": 20, "remaining_bytes": 1000, "truncated": false,
                    "truncation": {
                        "occurred": false, "reasons": [], "excerpt_count": 0,
                        "omitted_count": 0, "candidate_details_compacted": false,
                        "excerpts": [], "omissions": []
                    }
                },
                "missing_evidence": [],
                "reference_index": {
                    "candidates": [candidate_id], "paths": [], "findings": [],
                    "relationships": [], "clusters": [], "excerpts": [],
                    "policies": ["test-rule"], "verification": []
                }
            },
            "policies": {
                "resolution_digest": "d".repeat(64),
                "packs": [{
                    "id": "org.example.test", "version": "1.0.0", "schema_version": 1,
                    "source_type": "built-in", "source_revision": "4".repeat(64),
                    "content_digest": "4".repeat(64),
                    "entrypoints": [{"path": "policy.md", "sha256": "5".repeat(64)}]
                }],
                "conflicts": []
            },
            "provider": {
                "provider": "mock", "model": "test", "requested_runtime_model": "test",
                "endpoint_classification": "none", "reasoning_effort": "medium",
                "timeout_ms": 1, "max_response_bytes": 4096, "max_output_tokens": 128,
                "context_window_tokens": 2048, "runtime_label": null, "model_digest": null,
                "resource_preflight": null
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
                    "reported_verdict": "approve",
                    "aggregate_verdict": "approve",
                    "rationale": "test",
                    "citations": citations(false),
                    "rule_evaluations": [{
                        "rule_id": "test-rule", "verdict": "approve",
                        "rationale": "test", "citations": citations(true)
                    }],
                    "requested_revisions": [], "recommended_next_step": null,
                    "assumptions": [], "missing_evidence": [], "confidence": "high"
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
