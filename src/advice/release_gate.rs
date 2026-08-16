use anyhow::{Context, Result, bail};
use serde::Deserialize;

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
    pub maximum_swap_growth_bytes: u64,
    pub decision_record: String,
}

pub fn release_gate() -> Result<AdvisorReleaseGate> {
    let gate: AdvisorReleaseGate =
        serde_json::from_slice(include_bytes!("../../benchmarks/advisor/release-gate.json"))
            .context("embedded advisor release gate is invalid")?;
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
}
