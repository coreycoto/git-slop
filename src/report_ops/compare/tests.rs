#[cfg(test)]
mod tests {
    use super::*;

    fn record(path: &str, score: f64) -> Value {
        json!({
            "path": path,
            "content_fingerprint": format!("fingerprint-{path}"),
            "analysis_status": "analyzed",
            "tokens": 10,
            "context_band": "compact",
            "slop_score": score,
            "slop_band": if score >= 50.0 { "high" } else { "low" },
            "costs": {"load": {"load_pressure": score / 100.0}},
            "overlays": {}
        })
    }

    fn report(profile: &str, records: Vec<Value>) -> Value {
        let returned = records.len();
        let policy_records = records
            .iter()
            .map(|record| {
                json!({
                    "path": record.get("path"),
                    "classification": "source",
                    "profile": "agent_context",
                    "generated_from": [],
                    "tokens": record.get("tokens"),
                    "context_band": record.get("context_band"),
                    "slop_score": record.get("slop_score"),
                    "slop_band": record.get("slop_band"),
                    "reason_codes": []
                })
            })
            .collect::<Vec<_>>();
        json!({
            "schema_version": 5,
            "analyzer": {
                "report_profile": profile,
                "context_tokenizer": "cl100k_base",
                "analysis_config_digest": "analysis",
                "evidence_config_digest": "evidence",
                "analysis_contract_version": 2
            },
            "repo": {"repository_id": "repo"},
            "scope": {"mode": "repository", "path": null},
            "files": records.iter().take(250).cloned().collect::<Vec<_>>(),
            "folders": [],
            "compare_index": {"files": records, "folders": []},
            "policy_index": {"files": policy_records, "folders": []},
            "action_queue": [],
            "collection_metadata": {
                "compare_index": {
                    "files": {"total": returned, "returned": returned, "limit": null, "truncated": false},
                    "folders": {"total": 0, "returned": 0, "limit": null, "truncated": false}
                },
                "policy_index": {
                    "files": {"total": returned, "returned": returned, "limit": null, "truncated": false},
                    "folders": {"total": 0, "returned": 0, "limit": null, "truncated": false}
                }
            },
            "evidence_completeness": {"history": "complete"},
            "diagnostics": {"analysis": {"analysis_status": "complete"}}
        })
    }

    #[test]
    fn unchanged_compact_and_full_reports_compare_via_exhaustive_index() {
        let records = (0..300)
            .map(|index| record(&format!("src/{index:03}.rs"), index as f64 / 10.0))
            .collect::<Vec<_>>();
        let payload = compare_payload_with_options(
            &report("compact", records.clone()),
            &report("full_evidence", records),
            None,
            None,
            10,
            false,
            false,
        )
        .expect("cross-profile comparison");
        assert_eq!(payload["summary"]["files"]["added"], 0);
        assert_eq!(payload["summary"]["files"]["removed"], 0);
        assert_eq!(payload["summary"]["files"]["changed"], 0);
        assert_eq!(payload["summary"]["files"]["unchanged"], 300);
        assert_eq!(payload["baseline_compatible"], true);
        assert_eq!(
            payload["compatibility_mismatches"][0]["code"],
            "presentation_profile_mismatch"
        );
    }

    #[test]
    fn compact_rank_shift_does_not_create_phantom_additions_or_removals() {
        let base = (0..300)
            .map(|index| record(&format!("src/{index:03}.rs"), index as f64 / 10.0))
            .collect::<Vec<_>>();
        let mut head = base.clone();
        head[299] = record("src/299.rs", 99.0);
        let payload = compare_payload_with_options(
            &report("compact", base),
            &report("compact", head),
            None,
            None,
            10,
            false,
            false,
        )
        .expect("compact comparison");
        assert_eq!(payload["summary"]["files"]["added"], 0);
        assert_eq!(payload["summary"]["files"]["removed"], 0);
        assert_eq!(payload["summary"]["files"]["changed"], 1);
        assert_eq!(payload["summary"]["files"]["unchanged"], 299);
    }

    #[test]
    fn unique_content_fingerprint_is_reported_as_a_rename() {
        let mut old = record("src/old.rs", 20.0);
        let mut new = record("src/new.rs", 20.0);
        old["content_fingerprint"] = json!("same-content");
        new["content_fingerprint"] = json!("same-content");
        let payload = compare_payload_with_options(
            &report("standard", vec![old]),
            &report("standard", vec![new]),
            None,
            None,
            10,
            false,
            false,
        )
        .expect("rename comparison");
        assert_eq!(payload["summary"]["files"]["added"], 0);
        assert_eq!(payload["summary"]["files"]["removed"], 0);
        assert_eq!(payload["summary"]["files"]["renamed"], 1);
        assert_eq!(payload["file_deltas"][0]["renamed_from"], "src/old.rs");
        assert_eq!(payload["file_deltas"][0]["renamed_to"], "src/new.rs");
        assert_eq!(payload["regressions"].as_array().unwrap().len(), 0);
    }

    #[test]
    fn explicit_policy_source_selects_base_or_head_thresholds() {
        let mut base = report("standard", vec![record("src/lib.rs", 20.0)]);
        let mut changed = record("src/lib.rs", 23.0);
        changed["content_fingerprint"] = json!("changed-content");
        let mut head = report("standard", vec![changed]);
        base["config"] =
            json!({"check": {"regression_score_delta": 5.0, "fail_on_evidence_drift": false}});
        head["config"] =
            json!({"check": {"regression_score_delta": 2.0, "fail_on_evidence_drift": false}});
        let base_policy =
            compare_payload_with_policy(&base, &head, None, None, 10, false, false, "base")
                .unwrap();
        let head_policy =
            compare_payload_with_policy(&base, &head, None, None, 10, false, false, "head")
                .unwrap();
        assert_eq!(base_policy["policy_source"], "base");
        assert_eq!(base_policy["summary"]["regression_count"], 0);
        assert_eq!(head_policy["policy_source"], "head");
        assert_eq!(head_policy["summary"]["regression_count"], 1);
    }

    #[test]
    fn comparison_distinguishes_non_text_inventory_from_coverage_loss() {
        let mut binary = record("assets/image.png", 0.0);
        binary["analysis_status"] = json!("skipped");
        binary["skipped_reason"] = json!("binary");
        binary["content_fingerprint"] = json!("incomplete:binary:8");

        compare_payload_with_options(
            &report("standard", vec![binary.clone()]),
            &report("standard", vec![binary.clone()]),
            None,
            None,
            10,
            false,
            false,
        )
        .expect("non-text records are intentionally outside structural analysis");

        binary["skipped_reason"] = json!("large_file_limit");
        let error = compare_payload_with_options(
            &report("standard", vec![binary.clone()]),
            &report("standard", vec![binary]),
            None,
            None,
            10,
            false,
            false,
        )
        .expect_err("large-file coverage loss remains fail-closed");
        assert!(error.to_string().contains("inventory_evidence_incomplete"));
    }
}
