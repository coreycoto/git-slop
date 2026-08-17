use anyhow::{Context, Result, bail};
use serde::Deserialize;

pub const CANONICAL_ADVISOR_MODEL: &str = "openai/gpt-oss-safeguard-20b";
pub const MINIMUM_MODEL_SIZE_BYTES: u64 = 13_793_441_254;
pub const MINIMUM_ESTIMATED_PEAK_MEMORY_BYTES: u64 = 16 * 1024 * 1024 * 1024;
pub const MINIMUM_PHYSICAL_MEMORY_BYTES: u64 = 24 * 1024 * 1024 * 1024;
pub const MINIMUM_AVAILABLE_MEMORY_RESERVE_BYTES: u64 = 8 * 1024 * 1024 * 1024;
pub const MAXIMUM_INITIAL_SWAP_USED_BYTES: u64 = 256 * 1024 * 1024;
pub const MAXIMUM_SWAP_GROWTH_BYTES: u64 = 256 * 1024 * 1024;
pub const ADVISOR_DECISION_RECORD: &str = "docs/benchmarks/safeguard-v1-m2-air-decision.md";

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AdvisorReleaseGate {
    pub schema_version: u64,
    pub recommendation: String,
    pub public_inference_enabled: bool,
    pub canonical_model: String,
    pub minimum_model_size_bytes: u64,
    pub minimum_estimated_peak_memory_bytes: u64,
    pub minimum_physical_memory_bytes: u64,
    pub minimum_available_memory_reserve_bytes: u64,
    pub maximum_initial_swap_used_bytes: u64,
    pub maximum_swap_growth_bytes: u64,
    pub decision_record: String,
}

fn validate_release_gate(gate: &AdvisorReleaseGate) -> Result<()> {
    if gate.schema_version != 1 {
        bail!(
            "embedded advisor release gate uses unsupported schema {}",
            gate.schema_version
        );
    }
    if !matches!(gate.recommendation.as_str(), "ship" | "adjust" | "defer") {
        bail!(
            "embedded advisor release gate has invalid recommendation {:?}",
            gate.recommendation
        );
    }
    if gate.public_inference_enabled && gate.recommendation != "ship" {
        bail!(
            "embedded advisor release gate cannot enable public inference without a ship recommendation"
        );
    }
    if gate.canonical_model != CANONICAL_ADVISOR_MODEL
        || gate.minimum_model_size_bytes < MINIMUM_MODEL_SIZE_BYTES
        || gate.minimum_estimated_peak_memory_bytes < MINIMUM_ESTIMATED_PEAK_MEMORY_BYTES
        || gate.minimum_estimated_peak_memory_bytes < gate.minimum_model_size_bytes
        || gate.minimum_physical_memory_bytes < MINIMUM_PHYSICAL_MEMORY_BYTES
        || gate.minimum_available_memory_reserve_bytes < MINIMUM_AVAILABLE_MEMORY_RESERVE_BYTES
        || gate.maximum_initial_swap_used_bytes > MAXIMUM_INITIAL_SWAP_USED_BYTES
        || gate.maximum_swap_growth_bytes > MAXIMUM_SWAP_GROWTH_BYTES
    {
        bail!("embedded advisor release gate weakens the fixed V1 resource safety floor");
    }
    if gate.decision_record != ADVISOR_DECISION_RECORD {
        bail!("embedded advisor release gate must reference the canonical V1 decision record");
    }
    Ok(())
}

pub fn release_gate() -> Result<AdvisorReleaseGate> {
    let gate: AdvisorReleaseGate =
        serde_json::from_slice(include_bytes!("../../benchmarks/advisor/release-gate.json"))
            .context("embedded advisor release gate is invalid")?;
    validate_release_gate(&gate)?;
    Ok(gate)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checked_in_gate_fails_closed() {
        let gate = release_gate().expect("release gate");
        assert_eq!(gate.recommendation, "defer");
        assert!(!gate.public_inference_enabled);
        assert!(gate.minimum_physical_memory_bytes >= 24 * 1024 * 1024 * 1024);
    }

    #[test]
    fn embedded_gate_cannot_weaken_runtime_safety_floors() {
        let mut gate = release_gate().expect("release gate");
        gate.minimum_available_memory_reserve_bytes = 4 * 1024 * 1024 * 1024;
        assert!(validate_release_gate(&gate).is_err());

        let mut gate = release_gate().expect("release gate");
        gate.maximum_initial_swap_used_bytes = MAXIMUM_INITIAL_SWAP_USED_BYTES + 1;
        assert!(validate_release_gate(&gate).is_err());
    }
}
