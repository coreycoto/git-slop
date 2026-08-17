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

        let mut benchmark_receipt = json!({
            "schema_version": 1,
            "operation": "advisor-benchmark",
            "operation_code": "advisor_benchmark_completed",
            "status": "written",
            "benchmark_status": "complete",
            "review_evidence": {
                "status": "retained",
                "protocol": REVIEW_PROTOCOL,
                "warning": "Private review evidence is retained outside the repository."
            },
            "results_output": "results.json",
            "decision_output": "decision.md"
        });
        validate_operation_receipt(&benchmark_receipt).unwrap();
        benchmark_receipt["review_evidence"]["protocol"] = Value::Null;
        assert!(validate_operation_receipt(&benchmark_receipt).is_err());
    }
