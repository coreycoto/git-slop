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
                "builder_version": 1,
                "digest": "c".repeat(64),
                "limits": {
                    "maximum_context_bytes": 131072, "maximum_context_tokens": 8192,
                    "estimated_context_tokens": 100, "per_excerpt_bytes": 4096,
                    "maximum_files": 20, "remaining_bytes": 1000, "truncated": false
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
    fn completed_results_require_bound_blind_review_before_immutable_finalization() {
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
            repetitions: 3,
            full_matrix: true,
            prepare_only: false,
            review_output_dir: None,
        };
        let mut samples = Vec::new();
        for case in &corpus.cases {
            for effort in expected_efforts(true) {
                for context in expected_contexts(true, case.candidate_count) {
                    for repetition in 1..=options.repetitions {
                        let index = samples.len();
                        samples.push(
                            seal_sample(Sample {
                                case_id: case.id.clone(),
                                repository: case.repository.clone(),
                                scenario_tags: case.scenario_tags.clone(),
                                scenario: case.scenario.clone(),
                                candidate_count: case.candidate_count,
                                actual_candidate_count: Some(case.candidate_count),
                                report_sha256: reports[&case.repository].sha256.clone(),
                                artifact_sha256: Some(sha256(format!("artifact-{index}").as_bytes())),
                                sample_sha256: String::new(),
                                reasoning_effort: (*effort).to_string(),
                                context_token_limit: *context,
                                output_token_limit: case
                                    .candidate_count
                                    .saturating_mul(2_048)
                                    .min(8_192),
                                repetition,
                                phase: if index == 0 { "cold" } else { "warm" }.to_string(),
                                status: "valid".to_string(),
                                exit_code: Some(0),
                                total_elapsed_ms: 1,
                                peak_process_rss_bytes: Some(1),
                                system_available_memory_before_bytes: Some(u64::MAX),
                                system_available_memory_after_bytes: Some(u64::MAX),
                                system_available_memory_minimum_bytes: Some(u64::MAX),
                                swap_before_bytes: Some(0),
                                swap_after_bytes: Some(0),
                                swap_growth_bytes: Some(0),
                                context_elapsed_ms: Some(1),
                                provider_elapsed_ms: Some(1),
                                validation_elapsed_ms: Some(1),
                                time_to_validated_artifact_ms: Some(1),
                                model_load_duration_ns: Some(1),
                                prompt_eval_duration_ns: Some(1),
                                generation_duration_ns: Some(1),
                                input_tokens: Some(1),
                                output_tokens: Some(1),
                                prompt_tokens_per_second: Some(1.0),
                                output_tokens_per_second: Some(1.0),
                                reported_aggregate: Some(case.expected_aggregate.clone()),
                                expected_aggregate: case.expected_aggregate.clone(),
                                aggregate_match: true,
                                matched_rule_verdicts: high_severity_expectation_count(
                                    &case.expected_rule_verdicts,
                                ) * case.candidate_count,
                                expected_rule_verdicts: high_severity_expectation_count(
                                    &case.expected_rule_verdicts,
                                ) * case.candidate_count,
                                accepted_invalid_references: 0,
                                accepted_detector_truth_changes: 0,
                                citation_complete: true,
                                retry_count: 0,
                                failure_category: None,
                            })
                            .unwrap(),
                        );
                    }
                }
            }
        }
        let (results, decision) = write_outputs(
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
        let review_directory = temporary.path().join("private-review");
        fs::create_dir(&review_directory).unwrap();
        let mut review_entries = Vec::new();
        let review_advice = json!({
            "candidate_ids": ["candidate-review"],
            "context": {"digest": "private"},
            "policies": {"resolution_digest": "private"},
            "evaluation": {"summary": "Review this blinded advice."},
            "validation": {"status": "valid"},
            "boundary": "Advisory only."
        });
        for sample in samples.iter().filter(|sample| {
            sample.reasoning_effort == "low" && sample.context_token_limit == 8_192
        }) {
            record_review_artifact(
                &review_directory,
                &mut review_entries,
                sample,
                &review_advice,
            )
            .unwrap();
        }
        write_review_manifests(&review_directory, &review_entries, &results, true).unwrap();
        let review_manifest = review_directory.join("review-manifest.json");
        let source_digest = sha256(&fs::read(&results).unwrap());
        let manifest_digest = sha256(&fs::read(&review_manifest).unwrap());
        let rating = json!({
            "recommendation_usefulness": 5,
            "fact_interpretation_separation": 5,
            "scope_quality": 5,
            "verification_quality": 5,
            "actionability": 5,
            "unsupported_claim_count": 0
        });
        let ratings_by_review = review_entries
            .iter()
            .filter(|entry| entry.reasoning_effort == "low")
            .map(|entry| (entry.review_id.clone(), rating.clone()))
            .collect::<BTreeMap<_, _>>();
        let ratings_path = temporary.path().join("ratings.json");
        fs::write(
            &ratings_path,
            serde_json::to_string_pretty(&json!({
                "schema_version": 2,
                "protocol": REVIEW_PROTOCOL,
                "source_results_sha256": source_digest,
                "review_manifest_sha256": manifest_digest,
                "reviewers": [
                    {"reviewer_id": "reviewer-one", "independent": true, "blinded": true, "ratings": ratings_by_review},
                    {"reviewer_id": "reviewer-two", "independent": true, "blinded": true, "ratings": ratings_by_review}
                ]
            }))
            .unwrap()
                + "\n",
        )
        .unwrap();
        let finalized_results = temporary.path().join("finalized-results.json");
        let finalized_decision = temporary.path().join("finalized-decision.md");
        let finalize_options = FinalizeOptions {
            repo_root: root.to_path_buf(),
            corpus: corpus_path.clone(),
            thresholds: thresholds_path.clone(),
            results: results.clone(),
            review_manifest: review_manifest.clone(),
            ratings: ratings_path.clone(),
            output: finalized_results.clone(),
            decision_output: finalized_decision.clone(),
            apply: false,
        };
        let original_decision = fs::read(&decision).unwrap();
        let original_results = fs::read(&results).unwrap();
        let original_ratings = fs::read(&ratings_path).unwrap();

        let mut tampered: Value = serde_json::from_slice(&original_results).unwrap();
        tampered["summary"]["automatic_gates_passed"] = json!(false);
        fs::write(&results, serde_json::to_string_pretty(&tampered).unwrap() + "\n").unwrap();
        let tampered_bytes = fs::read(&results).unwrap();
        let error = finalize(&finalize_options)
            .expect_err("derived gate drift must fail before mutation");
        assert!(error.to_string().contains("automatic_gates_passed"));
        assert_eq!(fs::read(&results).unwrap(), tampered_bytes);
        fs::write(&results, &original_results).unwrap();

        let mut stale_sample: Value = serde_json::from_slice(&original_results).unwrap();
        stale_sample["samples"][0]["total_elapsed_ms"] = json!(2);
        fs::write(
            &results,
            serde_json::to_string_pretty(&stale_sample).unwrap() + "\n",
        )
        .unwrap();
        let error = finalize(&finalize_options)
            .expect_err("sample evidence digest drift must fail before mutation");
        assert!(error.to_string().contains("stale sample_sha256"));
        fs::write(&results, &original_results).unwrap();

        let mut truncated: Value = serde_json::from_slice(&original_results).unwrap();
        truncated["samples"].as_array_mut().unwrap().pop();
        fs::write(&results, serde_json::to_string_pretty(&truncated).unwrap() + "\n").unwrap();
        let error = finalize(&finalize_options)
            .expect_err("truncated sample evidence must fail before mutation");
        assert!(error.to_string().contains("sample matrix ended"));
        fs::write(&results, &original_results).unwrap();

        fs::write(&decision, "# Incomplete decision template\n").unwrap();
        let error = finalize(&finalize_options)
            .expect_err("invalid decision template must fail before mutation");
        assert!(error.to_string().contains("does not match its result evidence"));
        fs::write(&decision, &original_decision).unwrap();

        let first_review = review_directory.join(&review_entries[0].artifact_file);
        let original_review = fs::read(&first_review).unwrap();
        fs::write(&first_review, b"{}\n").unwrap();
        let error = finalize(&finalize_options)
            .expect_err("review artifact digest drift must fail before mutation");
        assert!(error.to_string().contains("review artifact digest drifted"));
        fs::write(&first_review, original_review).unwrap();

        let mut one_reviewer: Value = serde_json::from_slice(&original_ratings).unwrap();
        one_reviewer["reviewers"].as_array_mut().unwrap().pop();
        fs::write(
            &ratings_path,
            serde_json::to_string_pretty(&one_reviewer).unwrap() + "\n",
        )
        .unwrap();
        let error = finalize(&finalize_options)
            .expect_err("one reviewer cannot satisfy independent review");
        assert!(error.to_string().contains("schema 2"));
        fs::write(&ratings_path, &original_ratings).unwrap();

        let preview = finalize(&finalize_options).unwrap();
        assert_eq!(
            preview.receipt["operation_code"],
            "advisor_benchmark_finalize_preview_valid"
        );
        assert!(!finalized_results.exists());
        assert!(!finalized_decision.exists());
        assert_eq!(fs::read(&results).unwrap(), original_results);
        assert_eq!(fs::read(&decision).unwrap(), original_decision);

        let applied = finalize(&FinalizeOptions {
            apply: true,
            ..finalize_options.clone()
        })
        .unwrap();
        assert_eq!(
            applied.receipt["operation_code"],
            "advisor_benchmark_finalize_applied"
        );
        let finalized: Value = serde_json::from_slice(&fs::read(&finalized_results).unwrap()).unwrap();
        assert_eq!(finalized["recommendation"], "ship");
        assert_eq!(finalized["source_results_sha256"], source_digest);
        assert_eq!(finalized["review_manifest_sha256"], manifest_digest);
        assert_eq!(finalized["summary"]["manual_quality"]["reviewer_count"], 2);
        assert_eq!(
            finalized["summary"]["manual_quality"]["reviewer_scores"]
                .as_array()
                .map(Vec::len),
            Some(2)
        );
        assert_eq!(fs::read(&results).unwrap(), original_results);
        assert_eq!(fs::read(&decision).unwrap(), original_decision);
        let finalized_bytes = fs::read(&finalized_results).unwrap();
        let error = finalize(&FinalizeOptions {
            apply: true,
            ..finalize_options
        })
        .expect_err("immutable finalized outputs must not be overwritten");
        assert!(error.to_string().contains("already exists"));
        assert_eq!(fs::read(&finalized_results).unwrap(), finalized_bytes);
    }

    #[test]
    fn advisor_operation_receipts_bind_codes_to_exact_states() {
        let mut receipt = json!({
            "schema_version": 1,
            "operation": "advisor-benchmark-finalize",
            "operation_code": "advisor_benchmark_finalize_preview_valid",
            "status": "preview",
            "apply": false,
            "recommendation": "adjust",
            "source_results_sha256": "1".repeat(64),
            "review_manifest_sha256": "2".repeat(64),
            "manual_ratings_sha256": "3".repeat(64),
            "proposed_results_sha256": "4".repeat(64),
            "results_output": "finalized-results.json",
            "decision_output": "finalized-decision.md"
        });
        validate_operation_receipt(&receipt).unwrap();
        receipt["apply"] = json!(true);
        assert!(validate_operation_receipt(&receipt).is_err());
    }
}
