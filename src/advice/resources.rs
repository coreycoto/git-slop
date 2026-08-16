use std::fs;
use std::process::Command;

use anyhow::{Result, bail};
use serde::Serialize;

use super::AdvisorReleaseGate;

#[derive(Debug, Clone, Copy, Serialize)]
pub struct SystemResourceSnapshot {
    pub physical_memory_bytes: u64,
    pub available_memory_bytes: u64,
    pub swap_used_bytes: u64,
}

#[derive(Debug, Clone, Copy)]
pub struct RuntimeResourceGuard {
    pub minimum_available_memory_bytes: u64,
    pub maximum_swap_growth_bytes: u64,
    pub initial_swap_used_bytes: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ResourcePreflight {
    pub model_size_bytes: u64,
    pub estimated_peak_memory_bytes: u64,
    pub required_available_memory_bytes: u64,
    pub maximum_swap_growth_bytes: u64,
    pub system: SystemResourceSnapshot,
}

fn command_text(program: &str, args: &[&str]) -> Option<String> {
    let output = Command::new(program).args(args).output().ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

fn parse_size(value: &str) -> Option<u64> {
    let value = value.trim().trim_end_matches(',');
    let split = value
        .char_indices()
        .find(|(_, character)| !character.is_ascii_digit() && *character != '.')
        .map(|(index, _)| index)
        .unwrap_or(value.len());
    let number = value[..split].parse::<f64>().ok()?;
    let unit = value[split..].trim().to_ascii_uppercase();
    let multiplier = match unit.as_str() {
        "" | "B" => 1.0,
        "K" | "KB" | "KIB" => 1024.0,
        "M" | "MB" | "MIB" => 1024.0 * 1024.0,
        "G" | "GB" | "GIB" => 1024.0 * 1024.0 * 1024.0,
        _ => return None,
    };
    Some((number * multiplier) as u64)
}

fn linux_meminfo_value(name: &str) -> Option<u64> {
    fs::read_to_string("/proc/meminfo")
        .ok()?
        .lines()
        .find(|line| line.starts_with(name))?
        .split_whitespace()
        .nth(1)?
        .parse::<u64>()
        .ok()
        .map(|kilobytes| kilobytes.saturating_mul(1024))
}

pub fn physical_memory_bytes() -> Option<u64> {
    if cfg!(target_os = "macos") {
        return command_text("sysctl", &["-n", "hw.memsize"])
            .and_then(|value| value.parse::<u64>().ok());
    }
    linux_meminfo_value("MemTotal:")
}

pub fn available_memory_bytes() -> Option<u64> {
    if cfg!(target_os = "macos") {
        let output = command_text("vm_stat", &[])?;
        let page_size = output
            .lines()
            .next()?
            .split("page size of ")
            .nth(1)?
            .split_whitespace()
            .next()?
            .parse::<u64>()
            .ok()?;
        let pages = output
            .lines()
            .filter_map(|line| {
                let (name, value) = line.split_once(':')?;
                matches!(name, "Pages free" | "Pages inactive" | "Pages speculative")
                    .then(|| value.trim().trim_end_matches('.').parse::<u64>().ok())
                    .flatten()
            })
            .sum::<u64>();
        return (pages > 0).then_some(pages.saturating_mul(page_size));
    }
    linux_meminfo_value("MemAvailable:")
}

pub fn swap_used_bytes() -> Option<u64> {
    if cfg!(target_os = "macos") {
        let output = command_text("sysctl", &["-n", "vm.swapusage"])?;
        let used = output
            .split_whitespace()
            .skip_while(|part| *part != "used")
            .nth(2)?;
        return parse_size(used);
    }
    let total = linux_meminfo_value("SwapTotal:")?;
    let free = linux_meminfo_value("SwapFree:")?;
    Some(total.saturating_sub(free))
}

pub fn system_resource_snapshot() -> Result<SystemResourceSnapshot> {
    Ok(SystemResourceSnapshot {
        physical_memory_bytes: physical_memory_bytes().ok_or_else(|| {
            anyhow::anyhow!(
                "advisor_capacity_unknown: physical memory could not be measured on this host"
            )
        })?,
        available_memory_bytes: available_memory_bytes().ok_or_else(|| {
            anyhow::anyhow!(
                "advisor_capacity_unknown: available memory could not be measured on this host"
            )
        })?,
        swap_used_bytes: swap_used_bytes().ok_or_else(|| {
            anyhow::anyhow!("advisor_capacity_unknown: swap use could not be measured on this host")
        })?,
    })
}

fn validate_capacity(
    gate: &AdvisorReleaseGate,
    model_size_bytes: u64,
    estimated_peak_memory_bytes: u64,
    system: SystemResourceSnapshot,
) -> Result<ResourcePreflight> {
    if model_size_bytes < gate.minimum_model_size_bytes {
        bail!(
            "advisor_model_size_invalid: the canonical model requires at least {} bytes, received {model_size_bytes}",
            gate.minimum_model_size_bytes
        );
    }
    if estimated_peak_memory_bytes < gate.minimum_estimated_peak_memory_bytes
        || estimated_peak_memory_bytes < model_size_bytes
    {
        bail!(
            "advisor_peak_memory_invalid: estimated peak memory must be at least {} bytes and no smaller than the model artifact",
            gate.minimum_estimated_peak_memory_bytes
        );
    }
    let required_available_memory_bytes = estimated_peak_memory_bytes
        .checked_add(gate.minimum_available_memory_reserve_bytes)
        .ok_or_else(|| anyhow::anyhow!("advisor_capacity_invalid: memory requirement overflow"))?;
    let required_physical_memory_bytes = gate
        .minimum_physical_memory_bytes
        .max(required_available_memory_bytes);
    if system.physical_memory_bytes < required_physical_memory_bytes {
        bail!(
            "advisor_capacity_insufficient: host physical memory is {} bytes; this configuration requires at least {} bytes. Do not run it on this host",
            system.physical_memory_bytes,
            required_physical_memory_bytes
        );
    }
    if system.available_memory_bytes < required_available_memory_bytes {
        bail!(
            "advisor_headroom_insufficient: host available memory is {} bytes; this configuration requires at least {} bytes before provider contact. Free memory or use a separately provisioned host",
            system.available_memory_bytes,
            required_available_memory_bytes
        );
    }
    Ok(ResourcePreflight {
        model_size_bytes,
        estimated_peak_memory_bytes,
        required_available_memory_bytes,
        maximum_swap_growth_bytes: gate.maximum_swap_growth_bytes,
        system,
    })
}

pub fn preflight_resources(
    gate: &AdvisorReleaseGate,
    model_size_bytes: u64,
    estimated_peak_memory_bytes: u64,
) -> Result<(ResourcePreflight, RuntimeResourceGuard)> {
    let preflight = validate_capacity(
        gate,
        model_size_bytes,
        estimated_peak_memory_bytes,
        system_resource_snapshot()?,
    )?;
    let guard = RuntimeResourceGuard {
        minimum_available_memory_bytes: gate.minimum_available_memory_reserve_bytes,
        maximum_swap_growth_bytes: gate.maximum_swap_growth_bytes,
        initial_swap_used_bytes: preflight.system.swap_used_bytes,
    };
    Ok((preflight, guard))
}

pub fn enforce_resource_guard(guard: RuntimeResourceGuard) -> Result<()> {
    let available = available_memory_bytes().ok_or_else(|| {
        anyhow::anyhow!(
            "provider_resource_guard_unavailable: available memory could not be measured"
        )
    })?;
    if available < guard.minimum_available_memory_bytes {
        bail!(
            "provider_resource_guard_triggered: available memory fell to {available} bytes, below the {}-byte safety reserve; the request was aborted",
            guard.minimum_available_memory_bytes
        );
    }
    let swap = swap_used_bytes().ok_or_else(|| {
        anyhow::anyhow!("provider_resource_guard_unavailable: swap use could not be measured")
    })?;
    let growth = swap.saturating_sub(guard.initial_swap_used_bytes);
    if growth > guard.maximum_swap_growth_bytes {
        bail!(
            "provider_resource_guard_triggered: swap grew by {growth} bytes, above the {}-byte limit; the request was aborted",
            guard.maximum_swap_growth_bytes
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gate() -> AdvisorReleaseGate {
        super::super::release_gate().expect("release gate")
    }

    #[test]
    fn known_sixteen_gib_host_is_rejected() {
        let error = validate_capacity(
            &gate(),
            13_793_441_254,
            17_179_869_184,
            SystemResourceSnapshot {
                physical_memory_bytes: 16 * 1024 * 1024 * 1024,
                available_memory_bytes: 15 * 1024 * 1024 * 1024,
                swap_used_bytes: 0,
            },
        )
        .expect_err("16 GiB host must fail");
        assert!(error.to_string().contains("Do not run it on this host"));
    }

    #[test]
    fn adequately_resourced_host_is_accepted() {
        let preflight = validate_capacity(
            &gate(),
            13_793_441_254,
            17_179_869_184,
            SystemResourceSnapshot {
                physical_memory_bytes: 64 * 1024 * 1024 * 1024,
                available_memory_bytes: 48 * 1024 * 1024 * 1024,
                swap_used_bytes: 0,
            },
        )
        .expect("large host");
        assert_eq!(
            preflight.required_available_memory_bytes,
            24 * 1024 * 1024 * 1024
        );
    }
}
