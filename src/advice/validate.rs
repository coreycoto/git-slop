use std::collections::{BTreeMap, BTreeSet};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::policy::{CompiledPolicySet, Verdict, aggregate_verdict};

use super::ADVICE_RESPONSE_SCHEMA_VERSION;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Citations {
    pub candidates: Vec<String>,
    pub paths: Vec<String>,
    pub findings: Vec<String>,
    pub relationships: Vec<String>,
    pub clusters: Vec<String>,
    pub excerpts: Vec<String>,
    pub policies: Vec<String>,
    pub verification: Vec<String>,
}

impl Citations {
    fn count(&self) -> usize {
        self.candidates.len()
            + self.paths.len()
            + self.findings.len()
            + self.relationships.len()
            + self.clusters.len()
            + self.excerpts.len()
            + self.policies.len()
            + self.verification.len()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuleEvaluation {
    pub rule_id: String,
    pub verdict: Verdict,
    pub rationale: String,
    pub citations: Citations,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CandidateResponse {
    candidate_id: String,
    verdict: Verdict,
    rationale: String,
    rule_evaluations: Vec<RuleEvaluation>,
    citations: Citations,
    requested_revisions: Vec<String>,
    recommended_next_step: Option<String>,
    assumptions: Vec<String>,
    missing_evidence: Vec<String>,
    confidence: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ProviderResponse {
    schema_version: u64,
    aggregate_verdict: Verdict,
    summary: String,
    candidate_evaluations: Vec<CandidateResponse>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ValidatedCandidate {
    pub candidate_id: String,
    pub reported_verdict: Verdict,
    pub aggregate_verdict: Verdict,
    pub rationale: String,
    pub rule_evaluations: Vec<RuleEvaluation>,
    pub citations: Citations,
    pub requested_revisions: Vec<String>,
    pub recommended_next_step: Option<String>,
    pub assumptions: Vec<String>,
    pub missing_evidence: Vec<String>,
    pub confidence: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ValidatedResponse {
    pub schema_version: u64,
    pub reported_aggregate_verdict: Verdict,
    pub aggregate_verdict: Verdict,
    pub summary: String,
    pub candidate_evaluations: Vec<ValidatedCandidate>,
    pub warnings: Vec<String>,
}

fn reference_set(input: &Value, key: &str) -> BTreeSet<String> {
    input
        .pointer(&format!("/reference_index/{key}"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(ToOwned::to_owned)
        .collect()
}

fn validate_string(value: &str, label: &str, maximum: usize) -> Result<()> {
    let length = value.chars().count();
    if value.trim().is_empty() || length > maximum {
        bail!("{label} must contain 1 through {maximum} characters");
    }
    Ok(())
}

fn validate_string_list(values: &[String], label: &str, count: usize, length: usize) -> Result<()> {
    if values.len() > count {
        bail!("{label} exceeds the {count}-item limit");
    }
    for value in values {
        validate_string(value, label, length)?;
    }
    Ok(())
}

fn validate_references(
    citations: &Citations,
    references: &BTreeMap<&str, BTreeSet<String>>,
) -> Result<()> {
    if citations.count() == 0 {
        bail!("every policy rationale must cite at least one supplied reference");
    }
    for (kind, values) in [
        ("candidates", &citations.candidates),
        ("paths", &citations.paths),
        ("findings", &citations.findings),
        ("relationships", &citations.relationships),
        ("clusters", &citations.clusters),
        ("excerpts", &citations.excerpts),
        ("policies", &citations.policies),
        ("verification", &citations.verification),
    ] {
        if values.len() > 50 {
            bail!("citation collection {kind} exceeds the 50-item limit");
        }
        let known = references.get(kind).expect("reference kind exists");
        let mut seen = BTreeSet::new();
        for value in values {
            if !seen.insert(value) {
                bail!("duplicate {kind} citation {value:?}");
            }
            if !known.contains(value) {
                bail!("invented or unavailable {kind} citation {value:?}");
            }
        }
    }
    Ok(())
}

pub fn validate_response(
    raw: &Value,
    input: &Value,
    policies: &CompiledPolicySet,
) -> Result<ValidatedResponse> {
    let schema: Value = serde_json::from_str(include_str!("../../schemas/advice-response-1.json"))?;
    let validator = jsonschema::draft202012::options()
        .should_validate_formats(true)
        .build(&schema)
        .context("embedded advice-response schema is invalid")?;
    if let Some(error) = validator.iter_errors(raw).next() {
        bail!(
            "provider response does not match advice-response schema v1 at {}: {}",
            error.instance_path(),
            error
        );
    }
    let response: ProviderResponse = serde_json::from_value(raw.clone())
        .context("provider response does not match advice-response schema v1")?;
    if response.schema_version != ADVICE_RESPONSE_SCHEMA_VERSION {
        bail!(
            "unsupported provider advice-response schema {}",
            response.schema_version
        );
    }
    validate_string(&response.summary, "advice summary", 4000)?;
    let expected_candidates = reference_set(input, "candidates");
    let expected_rules = policies
        .rules
        .iter()
        .filter(|rule| rule.applicability.iter().any(|value| value == "advise"))
        .map(|rule| rule.id.clone())
        .collect::<BTreeSet<_>>();
    let references = [
        "candidates",
        "paths",
        "findings",
        "relationships",
        "clusters",
        "excerpts",
        "policies",
        "verification",
    ]
    .into_iter()
    .map(|key| (key, reference_set(input, key)))
    .collect::<BTreeMap<_, _>>();
    let mut observed_candidates = BTreeSet::new();
    let mut validated = Vec::new();
    let mut warnings = Vec::new();
    for candidate in response.candidate_evaluations {
        if !expected_candidates.contains(&candidate.candidate_id)
            || !observed_candidates.insert(candidate.candidate_id.clone())
        {
            bail!(
                "provider response contains an unknown or duplicate candidate {}",
                candidate.candidate_id
            );
        }
        validate_string(&candidate.rationale, "candidate rationale", 4000)?;
        validate_string_list(
            &candidate.requested_revisions,
            "requested revision",
            20,
            1000,
        )?;
        validate_string_list(&candidate.assumptions, "assumption", 20, 1000)?;
        validate_string_list(&candidate.missing_evidence, "missing evidence", 20, 1000)?;
        if let Some(step) = &candidate.recommended_next_step {
            validate_string(step, "recommended next step", 2000)?;
        }
        if !["low", "medium", "high"].contains(&candidate.confidence.as_str()) {
            bail!("candidate confidence must be low, medium, or high");
        }
        validate_references(&candidate.citations, &references)?;
        let mut observed_rules = BTreeSet::new();
        for rule in &candidate.rule_evaluations {
            if !expected_rules.contains(&rule.rule_id)
                || !observed_rules.insert(rule.rule_id.clone())
            {
                bail!(
                    "candidate {} contains an unknown or duplicate rule evaluation {}",
                    candidate.candidate_id,
                    rule.rule_id
                );
            }
            validate_string(&rule.rationale, "rule rationale", 2000)?;
            validate_references(&rule.citations, &references)?;
            if !rule.citations.policies.contains(&rule.rule_id) {
                bail!(
                    "rule evaluation {} must cite its own supplied policy ID",
                    rule.rule_id
                );
            }
        }
        if observed_rules != expected_rules {
            let missing = expected_rules
                .difference(&observed_rules)
                .cloned()
                .collect::<Vec<_>>();
            bail!(
                "candidate {} is missing rule evaluations: {}",
                candidate.candidate_id,
                missing.join(", ")
            );
        }
        let aggregate =
            aggregate_verdict(candidate.rule_evaluations.iter().map(|rule| rule.verdict));
        if aggregate != candidate.verdict {
            warnings.push(format!(
                "candidate {} reported {} but deterministic aggregation is {}",
                candidate.candidate_id,
                candidate.verdict.as_str(),
                aggregate.as_str()
            ));
        }
        validated.push(ValidatedCandidate {
            candidate_id: candidate.candidate_id,
            reported_verdict: candidate.verdict,
            aggregate_verdict: aggregate,
            rationale: candidate.rationale,
            rule_evaluations: candidate.rule_evaluations,
            citations: candidate.citations,
            requested_revisions: candidate.requested_revisions,
            recommended_next_step: candidate.recommended_next_step,
            assumptions: candidate.assumptions,
            missing_evidence: candidate.missing_evidence,
            confidence: candidate.confidence,
        });
    }
    if observed_candidates != expected_candidates {
        let missing = expected_candidates
            .difference(&observed_candidates)
            .cloned()
            .collect::<Vec<_>>();
        bail!(
            "provider response is missing candidate evaluations: {}",
            missing.join(", ")
        );
    }
    validated.sort_by(|left, right| left.candidate_id.cmp(&right.candidate_id));
    let aggregate = aggregate_verdict(
        validated
            .iter()
            .map(|candidate| candidate.aggregate_verdict),
    );
    if aggregate != response.aggregate_verdict {
        warnings.push(format!(
            "provider reported aggregate {} but deterministic aggregation is {}",
            response.aggregate_verdict.as_str(),
            aggregate.as_str()
        ));
    }
    Ok(ValidatedResponse {
        schema_version: ADVICE_RESPONSE_SCHEMA_VERSION,
        reported_aggregate_verdict: response.aggregate_verdict,
        aggregate_verdict: aggregate,
        summary: response.summary,
        candidate_evaluations: validated,
        warnings,
    })
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use tempfile::tempdir;

    use super::*;

    fn policies(root: &std::path::Path) -> CompiledPolicySet {
        crate::policy::resolve_for_advice(root, &[]).expect("core policies")
    }

    fn input(policies: &CompiledPolicySet) -> Value {
        json!({
            "reference_index": {
                "candidates": ["candidate-0123456789abcdef"],
                "paths": ["src/lib.rs"],
                "findings": ["finding-1"],
                "relationships": [],
                "clusters": [],
                "excerpts": ["excerpt-0123456789abcdef"],
                "policies": policies.rules.iter().map(|rule| rule.id.as_str()).collect::<Vec<_>>(),
                "verification": ["cargo test"]
            }
        })
    }

    fn citations(candidate: &str, policy: Option<&str>) -> Value {
        json!({
            "candidates": [candidate], "paths": [], "findings": [],
            "relationships": [], "clusters": [], "excerpts": [],
            "policies": policy.into_iter().collect::<Vec<_>>(), "verification": []
        })
    }

    fn response(policies: &CompiledPolicySet) -> Value {
        let candidate = "candidate-0123456789abcdef";
        json!({
            "schema_version": 1,
            "aggregate_verdict": "approve",
            "summary": "The candidate is bounded by the supplied policy set.",
            "candidate_evaluations": [{
                "candidate_id": candidate,
                "verdict": "approve",
                "rationale": "Every material conclusion cites supplied evidence.",
                "rule_evaluations": policies.rules.iter().filter(|rule| rule.applicability.iter().any(|value| value == "advise")).map(|rule| json!({
                    "rule_id": rule.id,
                    "verdict": "approve",
                    "rationale": "The candidate satisfies the cited rule.",
                    "citations": citations(candidate, Some(&rule.id))
                })).collect::<Vec<_>>(),
                "citations": citations(candidate, None),
                "requested_revisions": [], "recommended_next_step": null,
                "assumptions": [], "missing_evidence": [], "confidence": "high"
            }]
        })
    }

    #[test]
    fn valid_response_is_accepted_and_aggregate_is_recomputed() {
        let root = tempdir().expect("temporary root");
        let policies = policies(root.path());
        let validated = validate_response(&response(&policies), &input(&policies), &policies)
            .expect("valid response");
        assert_eq!(validated.aggregate_verdict, Verdict::Approve);
        assert!(validated.warnings.is_empty());
    }

    #[test]
    fn invented_reference_and_unknown_field_fail_closed() {
        let root = tempdir().expect("temporary root");
        let policies = policies(root.path());
        let input = input(&policies);
        let mut invented = response(&policies);
        invented["candidate_evaluations"][0]["citations"]["paths"] = json!(["src/invented.rs"]);
        assert!(
            validate_response(&invented, &input, &policies)
                .expect_err("invented path")
                .to_string()
                .contains("invented or unavailable paths")
        );
        let mut unknown = response(&policies);
        unknown["unexpected"] = json!(true);
        assert!(
            validate_response(&unknown, &input, &policies)
                .expect_err("unknown field")
                .to_string()
                .contains("does not match advice-response schema")
        );
    }

    #[test]
    fn reported_verdict_cannot_override_deterministic_aggregation() {
        let root = tempdir().expect("temporary root");
        let policies = policies(root.path());
        let input = input(&policies);
        let mut raw = response(&policies);
        raw["candidate_evaluations"][0]["rule_evaluations"][0]["verdict"] = json!("reject");
        let validated = validate_response(&raw, &input, &policies).expect("valid mismatch");
        assert_eq!(validated.aggregate_verdict, Verdict::Reject);
        assert_eq!(validated.reported_aggregate_verdict, Verdict::Approve);
        assert_eq!(validated.warnings.len(), 2);
    }
}
