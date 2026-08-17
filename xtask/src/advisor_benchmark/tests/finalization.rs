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
