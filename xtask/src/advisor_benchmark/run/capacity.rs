#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct BenchmarkReleaseGate {
    pub(super) schema_version: u64,
    pub(super) recommendation: String,
    pub(super) public_inference_enabled: bool,
    pub(super) canonical_model: String,
    pub(super) minimum_model_size_bytes: u64,
    pub(super) minimum_estimated_peak_memory_bytes: u64,
    pub(super) minimum_physical_memory_bytes: u64,
    pub(super) minimum_available_memory_reserve_bytes: u64,
    pub(super) maximum_initial_swap_used_bytes: u64,
    pub(super) maximum_swap_growth_bytes: u64,
    pub(super) decision_record: String,
}

pub(super) fn validate_benchmark_gate(gate: &BenchmarkReleaseGate) -> Result<()> {
    if gate.schema_version != 1
        || !matches!(gate.recommendation.as_str(), "ship" | "adjust" | "defer")
        || (gate.public_inference_enabled && gate.recommendation != "ship")
        || gate.canonical_model != "openai/gpt-oss-safeguard-20b"
        || gate.minimum_model_size_bytes < 13_793_441_254
        || gate.minimum_estimated_peak_memory_bytes < 16 * 1024 * 1024 * 1024
        || gate.minimum_estimated_peak_memory_bytes < gate.minimum_model_size_bytes
        || gate.minimum_physical_memory_bytes < 24 * 1024 * 1024 * 1024
        || gate.minimum_available_memory_reserve_bytes < 8 * 1024 * 1024 * 1024
        || gate.maximum_initial_swap_used_bytes > 256 * 1024 * 1024
        || gate.maximum_swap_growth_bytes > 256 * 1024 * 1024
        || gate.decision_record != "docs/benchmarks/safeguard-v1-m2-air-decision.md"
    {
        bail!("advisor release gate weakens the fixed V1 safety contract");
    }
    Ok(())
}

pub(super) fn validate_benchmark_capacity(
    gate: &BenchmarkReleaseGate,
    model_size: u64,
    estimated_peak: u64,
    physical: u64,
    available: u64,
    swap: u64,
) -> Result<BenchmarkWatchdog> {
    let (_, _, blockers) =
        benchmark_capacity_blockers(gate, model_size, estimated_peak, physical, available, swap)?;
    if !blockers.is_empty() {
        bail!(
            "{}",
            blockers
                .iter()
                .map(|blocker| blocker.message.as_str())
                .collect::<Vec<_>>()
                .join("; ")
        );
    }
    Ok(BenchmarkWatchdog {
        minimum_available_memory_bytes: gate.minimum_available_memory_reserve_bytes,
        maximum_swap_growth_bytes: gate.maximum_swap_growth_bytes,
        initial_swap_used_bytes: swap,
    })
}

pub(super) fn benchmark_capacity_blockers(
    gate: &BenchmarkReleaseGate,
    model_size: u64,
    estimated_peak: u64,
    physical: u64,
    available: u64,
    swap: u64,
) -> Result<(u64, u64, Vec<CapacityBlocker>)> {
    let required_available = estimated_peak
        .checked_add(gate.minimum_available_memory_reserve_bytes)
        .ok_or_else(|| anyhow::anyhow!("benchmark memory requirement overflow"))?;
    let required_physical = gate.minimum_physical_memory_bytes.max(required_available);
    let mut blockers = Vec::new();
    if model_size < gate.minimum_model_size_bytes {
        blockers.push(CapacityBlocker {
            code: "model_size_below_minimum",
            message: format!(
                "--model-size-bytes is {model_size}; the pinned canonical artifact requires at least {} bytes",
                gate.minimum_model_size_bytes
            ),
            actual_bytes: model_size,
            comparison: "minimum",
            limit_bytes: gate.minimum_model_size_bytes,
        });
    }
    if estimated_peak < gate.minimum_estimated_peak_memory_bytes {
        blockers.push(CapacityBlocker {
            code: "estimated_peak_memory_below_minimum",
            message: format!(
                "--estimated-peak-memory-bytes is {estimated_peak}; the safety floor requires at least {} bytes",
                gate.minimum_estimated_peak_memory_bytes
            ),
            actual_bytes: estimated_peak,
            comparison: "minimum",
            limit_bytes: gate.minimum_estimated_peak_memory_bytes,
        });
    }
    if estimated_peak < model_size {
        blockers.push(CapacityBlocker {
            code: "estimated_peak_memory_below_model_size",
            message: format!(
                "--estimated-peak-memory-bytes is {estimated_peak}; it cannot be smaller than the {model_size}-byte model artifact"
            ),
            actual_bytes: estimated_peak,
            comparison: "minimum",
            limit_bytes: model_size,
        });
    }
    if physical < required_physical {
        blockers.push(CapacityBlocker {
            code: "physical_memory_below_required",
            message: format!(
                "benchmark host has {physical} bytes of physical memory but requires at least {required_physical}; do not run on this host"
            ),
            actual_bytes: physical,
            comparison: "minimum",
            limit_bytes: required_physical,
        });
    }
    if available < required_available {
        blockers.push(CapacityBlocker {
            code: "available_memory_below_required",
            message: format!(
                "benchmark host has {available} bytes available but requires at least {required_available}; free capacity or use another dedicated host"
            ),
            actual_bytes: available,
            comparison: "minimum",
            limit_bytes: required_available,
        });
    }
    if swap > gate.maximum_initial_swap_used_bytes {
        blockers.push(CapacityBlocker {
            code: "initial_swap_above_maximum",
            message: format!(
                "benchmark host has {swap} bytes of swap in use but permits at most {}; recover memory pressure before any provider contact",
                gate.maximum_initial_swap_used_bytes
            ),
            actual_bytes: swap,
            comparison: "maximum",
            limit_bytes: gate.maximum_initial_swap_used_bytes,
        });
    }
    Ok((required_available, required_physical, blockers))
}

fn benchmark_safety_preflight(options: &Options) -> Result<BenchmarkWatchdog> {
    if !options.confirm_dedicated_host {
        bail!(
            "inference requires --confirm-dedicated-host; never run this matrix on a personal or capacity-constrained workstation"
        );
    }
    let gate_path = options
        .repo_root
        .join("benchmarks/advisor/release-gate.json");
    let gate: BenchmarkReleaseGate = serde_json::from_slice(&read_bounded(
        &gate_path,
        MAX_BENCHMARK_CONFIG_BYTES,
        "advisor release gate",
    )?)?;
    validate_benchmark_gate(&gate)
        .with_context(|| format!("advisor release gate is invalid: {}", gate_path.display()))?;
    if options.model != gate.canonical_model {
        bail!(
            "--model must explicitly equal the release-gated canonical model {}",
            gate.canonical_model
        );
    }
    let model_size = options
        .model_size_bytes
        .ok_or_else(|| anyhow::anyhow!("inference requires explicit --model-size-bytes"))?;
    let estimated_peak = options.estimated_peak_memory_bytes.ok_or_else(|| {
        anyhow::anyhow!("inference requires explicit --estimated-peak-memory-bytes")
    })?;
    let required_available = estimated_peak
        .checked_add(gate.minimum_available_memory_reserve_bytes)
        .ok_or_else(|| anyhow::anyhow!("benchmark memory requirement overflow"))?;
    let physical = system_physical_memory_bytes()
        .ok_or_else(|| anyhow::anyhow!("unable to measure physical memory; refusing inference"))?;
    let available = system_available_memory_bytes()
        .ok_or_else(|| anyhow::anyhow!("unable to measure available memory; refusing inference"))?;
    let swap = swap_used_bytes()
        .ok_or_else(|| anyhow::anyhow!("unable to measure swap use; refusing inference"))?;
    let watchdog =
        validate_benchmark_capacity(&gate, model_size, estimated_peak, physical, available, swap)?;
    eprintln!(
        "Advisor benchmark safety contract: dedicated-host-confirmed=true; model={} bytes; estimated peak={estimated_peak} bytes; required available={required_available} bytes; physical={physical} bytes; available={available} bytes; swap={swap} bytes; maximum initial swap={} bytes; maximum swap growth={} bytes; release recommendation={}; decision={}",
        model_size,
        gate.maximum_initial_swap_used_bytes,
        gate.maximum_swap_growth_bytes,
        gate.recommendation,
        gate.decision_record,
    );
    Ok(watchdog)
}

pub fn check_capacity(options: &CapacityCheckOptions) -> Result<CapacityCheck> {
    let gate_path = options
        .repo_root
        .join("benchmarks/advisor/release-gate.json");
    let gate: BenchmarkReleaseGate = serde_json::from_slice(&read_bounded(
        &gate_path,
        MAX_BENCHMARK_CONFIG_BYTES,
        "advisor release gate",
    )?)?;
    validate_benchmark_gate(&gate)
        .with_context(|| format!("advisor release gate is invalid: {}", gate_path.display()))?;
    if options.model != gate.canonical_model {
        bail!(
            "--model must explicitly equal the release-gated canonical model {}",
            gate.canonical_model
        );
    }
    let physical = system_physical_memory_bytes()
        .ok_or_else(|| anyhow::anyhow!("unable to measure physical memory; capacity is unknown"))?;
    let available = system_available_memory_bytes().ok_or_else(|| {
        anyhow::anyhow!("unable to measure available memory; capacity is unknown")
    })?;
    let swap = swap_used_bytes()
        .ok_or_else(|| anyhow::anyhow!("unable to measure swap use; capacity is unknown"))?;
    let (required_available, required_physical, blockers) = benchmark_capacity_blockers(
        &gate,
        options.model_size_bytes,
        options.estimated_peak_memory_bytes,
        physical,
        available,
        swap,
    )?;
    let eligible = blockers.is_empty();
    Ok(CapacityCheck {
        eligible,
        receipt: json!({
            "schema_version": 1,
            "command": "advisor-capacity",
            "eligible": eligible,
            "provider_contacted": false,
            "report_accessed": false,
            "model": options.model,
            "model_size_bytes": options.model_size_bytes,
            "estimated_peak_memory_bytes": options.estimated_peak_memory_bytes,
            "required_available_memory_bytes": required_available,
            "required_physical_memory_bytes": required_physical,
            "host": {
                "physical_memory_bytes": physical,
                "available_memory_bytes": available,
                "swap_used_bytes": swap,
            },
            "limits": {
                "minimum_model_size_bytes": gate.minimum_model_size_bytes,
                "minimum_estimated_peak_memory_bytes": gate.minimum_estimated_peak_memory_bytes,
                "minimum_physical_memory_bytes": gate.minimum_physical_memory_bytes,
                "minimum_available_memory_reserve_bytes": gate.minimum_available_memory_reserve_bytes,
                "maximum_initial_swap_used_bytes": gate.maximum_initial_swap_used_bytes,
                "maximum_swap_growth_bytes": gate.maximum_swap_growth_bytes,
            },
            "release": {
                "recommendation": gate.recommendation,
                "public_inference_enabled": gate.public_inference_enabled,
                "decision_record": gate.decision_record,
            },
            "blockers": blockers,
        }),
    })
}
