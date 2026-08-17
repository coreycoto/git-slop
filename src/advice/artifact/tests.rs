#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn human_advice_is_decision_ready_and_warns_about_private_retention() {
        let artifact = json!({
            "report": {"sha256": "report", "head_sha": "revision"},
            "context": {"digest": "context"},
            "policies": {"resolution_digest": "policies"},
            "provider": {"provider": "mock", "model": "model", "endpoint_classification": "none"},
            "evaluation": {
                "aggregate_verdict": "revise",
                "summary": "One bounded revision is required.",
                "candidate_evaluations": [{
                    "candidate_id": "candidate-0123456789abcdef",
                    "aggregate_verdict": "revise",
                    "rationale": "A cited test is missing.",
                    "confidence": "low",
                    "citations": {
                        "candidates": ["candidate-0123456789abcdef"],
                        "paths": ["src/lib.rs"], "findings": [], "relationships": [],
                        "clusters": [], "excerpts": [], "policies": ["policy.test"],
                        "verification": ["cargo test"]
                    },
                    "rule_evaluations": [{
                        "rule_id": "policy.test", "verdict": "revise",
                        "rationale": "Verification is required.",
                        "citations": {
                            "candidates": ["candidate-0123456789abcdef"],
                            "paths": [], "findings": [], "relationships": [], "clusters": [],
                            "excerpts": [], "policies": ["policy.test"], "verification": ["cargo test"]
                        }
                    }],
                    "requested_revisions": ["Add the missing test."],
                    "recommended_next_step": "Run the focused test after revising.",
                    "assumptions": ["The cited path remains current."],
                    "missing_evidence": ["Focused test result"]
                }]
            },
            "validation": {"warnings": []},
            "boundary": "Advice is advisory only."
        });

        let markdown = render_advice_markdown(&artifact);
        for expected in [
            "## Decision",
            "Candidate verdicts: 0 approve, 0 abstain, 1 revise, 0 reject",
            "Required revision items: 1",
            "Missing evidence items: 1",
            "Low-confidence candidates: 1",
            "Private retention:",
            "Confidence: **low**",
            "### Evidence citations",
            "<code>src/lib.rs</code>",
            "### Assumptions",
            "The cited path remains current.",
            "### Recommended next step",
        ] {
            assert!(
                markdown.contains(expected),
                "missing {expected:?}\n{markdown}"
            );
        }
    }
}
