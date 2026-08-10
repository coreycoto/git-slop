#[cfg(test)]
mod tests {
    use std::sync::{Arc, Barrier};

    use serde_json::json;
    use tempfile::tempdir;
    use tiktoken_rs::{cl100k_base, r50k_base};

    use super::{
        CachedTokenData, TokenCache, action_queue, configured_context_encoder, quarantine_cache,
        replace_quoted_strings, structural_tokens,
    };
    use crate::model::FileAnalysis;
    use crate::scoring;

    fn file(path: &str, relative_churn: f64) -> FileAnalysis {
        FileAnalysis {
            path: path.to_string(),
            bytes: 400,
            lines: 100,
            blank_lines: 0,
            code_lines: 100,
            comment_lines: 0,
            language: "Rust".to_string(),
            profile: "agent_context".to_string(),
            classification: "source".to_string(),
            analysis_status: "analyzed".to_string(),
            skipped_reason: None,
            symlink_metadata: None,
            has_inline_tests: false,
            tokens: 100,
            context_band: "compact".to_string(),
            context_pressure: 0.0,
            content_fingerprint: String::new(),
            structural_tokens: Vec::new(),
            structural_token_count: 0,
            top_structural_terms: Vec::new(),
            structural_categories: json!({"mode": "code"}),
            age_days: 0,
            revisions_window: 1,
            recency_weighted_commits: 0.0,
            added_window: 0,
            deleted_window: 0,
            churn_lines_window: 0,
            line_churn_window: 0,
            token_churn_window: 0,
            relative_churn_window: relative_churn,
            late_churn_spike: 0.0,
            author_count_window: 0,
            author_entropy: 0.0,
            top_author_share: 0.0,
            days_since_non_bot_edit: None,
            recent_maintainer_diversity: 0,
            age_pressure: 0.0,
            revision_norm: 0.0,
            relative_churn_norm: 0.0,
            churn_pressure: 0.0,
            slop_score: 0.0,
            slop_band: String::new(),
            reason_codes: Vec::new(),
            costs: json!({}),
            overlays: json!({}),
        }
    }

    #[test]
    fn structural_normalization_is_deterministic() {
        let tokens = structural_tokens(
            "src/my_file.rs",
            "let camelCase = \"secret 123\"; // hello-world",
        );
        assert!(tokens.contains(&"camel".to_string()));
        assert!(tokens.contains(&"case".to_string()));
        assert!(tokens.contains(&"str".to_string()));
        assert!(tokens.contains(&"my".to_string()));
        assert_eq!(
            replace_quoted_strings("'one' \"two\" `three`"),
            " str   str   str "
        );
    }

    #[test]
    fn structural_normalization_preserves_unicode_and_apostrophe_words() {
        let tokens = structural_tokens("docs/café.md", "L’équipe can’t rename HTTPServer_value");
        assert!(tokens.contains(&"équipe".to_string()));
        assert!(tokens.contains(&"can't".to_string()));
        assert!(tokens.contains(&"http".to_string()));
        assert!(tokens.contains(&"server".to_string()));
        assert!(tokens.contains(&"value".to_string()));
        assert!(tokens.iter().any(|token| token.contains("café")));
    }

    #[test]
    fn configured_tokenizer_is_used_exactly_and_unknown_names_fail_closed() {
        let text = "お誕生日おめでとう";
        let configured = configured_context_encoder(&json!({"tokenization": {
            "context_tokenizer_name": "r50k_base"
        }}))
        .unwrap();
        assert_eq!(
            configured.encode_ordinary(text).len(),
            r50k_base().unwrap().encode_ordinary(text).len()
        );
        assert_ne!(
            configured.encode_ordinary(text).len(),
            cl100k_base().unwrap().encode_ordinary(text).len()
        );

        let result = configured_context_encoder(&json!({"tokenization": {
            "context_tokenizer_name": "not-a-real-encoding"
        }}));
        let error = match result {
            Ok(_) => panic!("unsupported tokenizer must fail closed"),
            Err(error) => error,
        };
        assert!(
            error
                .to_string()
                .contains("unsupported tokenization.context_tokenizer_name")
        );
    }

    #[test]
    fn action_queue_prioritizes_line_relative_churn_signal() {
        let mut files = vec![file("src/quiet.rs", 0.1), file("src/volatile.rs", 2.0)];
        for file in &mut files {
            file.revisions_window = 5;
        }
        scoring::apply_scoring(&mut files, &json!({}));
        let queue = action_queue(&files, true, &json!({}));

        assert_eq!(queue[0]["path"], "src/volatile.rs");
        assert_eq!(queue[0]["reason_codes"][1], "high_relative_churn");
        assert_eq!(queue[0]["is_pure_context_hotspot"], false);
    }

    #[test]
    fn per_profile_queue_policy_can_suppress_low_score_data_context_noise() {
        let mut agent = file("src/lib.rs", 0.1);
        agent.reason_codes = vec!["high_token_cost".to_string()];
        agent.slop_score = 10.0;
        let mut data = file("fixtures/data.json", 0.1);
        data.profile = "data_context".to_string();
        data.reason_codes = vec!["high_token_cost".to_string()];
        data.slop_score = 10.0;
        let mut config = crate::config::default_config();
        config["health"]["profile_threshold_policy"] = json!("per_profile");
        let queue = action_queue(&[agent, data], true, &config);
        assert_eq!(queue.len(), 1);
        assert_eq!(queue[0]["path"], "src/lib.rs");
    }

    #[test]
    fn generated_and_non_product_evidence_never_enters_the_action_queue() {
        for classification in ["generated", "vendored", "snapshot", "migration_fixture"] {
            let mut candidate = file("generated/output.yml", 2.0);
            candidate.classification = classification.to_string();
            candidate.reason_codes = vec!["critical_token_cost".to_string()];
            candidate.slop_score = 100.0;
            candidate.slop_band = "critical".to_string();
            candidate.context_band = "critical".to_string();
            assert!(action_queue(&[candidate], true, &json!({})).is_empty());
        }
    }

    #[test]
    fn corrupt_token_cache_is_quarantined_instead_of_blocking_analysis() {
        let root = tempdir().unwrap();
        let path = root.path().join("token-v4.sqlite3");
        std::fs::write(&path, b"not a sqlite database").unwrap();
        let error = match TokenCache::open(&path) {
            Ok(_) => panic!("corrupt cache must fail validation"),
            Err(error) => error,
        };
        let warning = quarantine_cache(&path, &error);
        assert!(warning.contains("continued uncached"));
        assert!(!path.exists());
        assert!(
            std::fs::read_dir(root.path())
                .unwrap()
                .flatten()
                .any(|entry| entry.file_name().to_string_lossy().contains("corrupt-"))
        );
    }

    #[test]
    fn token_cache_accepts_concurrent_writers_with_busy_timeout() {
        let root = tempdir().unwrap();
        let path = Arc::new(root.path().join("token-v4.sqlite3"));
        TokenCache::open(path.as_ref()).unwrap();
        let barrier = Arc::new(Barrier::new(4));
        let handles = (0..4)
            .map(|worker| {
                let path = Arc::clone(&path);
                let barrier = Arc::clone(&barrier);
                std::thread::spawn(move || {
                    let cache = TokenCache::open(path.as_ref()).unwrap();
                    barrier.wait();
                    for item in 0..10 {
                        cache
                            .put(
                                &format!("worker-{worker}-item-{item}"),
                                &CachedTokenData {
                                    token_count: item,
                                    structural_tokens: vec![format!("token-{item}")],
                                    content_fingerprint: format!("fingerprint-{worker}-{item}"),
                                },
                            )
                            .unwrap();
                    }
                })
            })
            .collect::<Vec<_>>();
        for handle in handles {
            handle.join().unwrap();
        }
        assert_eq!(
            TokenCache::open(&path).unwrap().stats().unwrap().entries,
            40
        );
    }
}
