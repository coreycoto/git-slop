use anyhow::Result;
use serde::Serialize;
use serde_json::json;

#[derive(Debug, Clone, Serialize)]
pub(super) struct GateReceipt {
    pub name: &'static str,
    pub status: &'static str,
    pub elapsed_ms: u128,
}

#[derive(Debug, Clone, Serialize)]
pub(super) struct PrerequisiteReceipt {
    pub name: String,
    pub status: &'static str,
    pub detail: String,
    pub recovery: Option<String>,
}

pub(super) fn bounded_output(bytes: &[u8]) -> String {
    const MAX_CHARS: usize = 12_000;
    let value = String::from_utf8_lossy(bytes);
    let trimmed = value.trim();
    if trimmed.chars().count() <= MAX_CHARS {
        return trimmed.to_string();
    }
    let tail = trimmed
        .chars()
        .rev()
        .take(MAX_CHARS)
        .collect::<String>()
        .chars()
        .rev()
        .collect::<String>();
    format!("[earlier output truncated]\n{tail}")
}

pub(super) fn print_ci(
    status: &str,
    gates: &[GateReceipt],
    prerequisites: &[PrerequisiteReceipt],
    elapsed_ms: u128,
    failed_gate: Option<&str>,
    error: Option<&str>,
) -> Result<()> {
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "schema_version": 2,
            "command": "ci",
            "status": status,
            "gates": gates,
            "selected_gates": gates.iter().map(|gate| gate.name).collect::<Vec<_>>(),
            "skipped_gates": [],
            "prerequisites": prerequisites,
            "elapsed_ms": elapsed_ms,
            "failed_gate": failed_gate,
            "error": error
        }))?
    );
    Ok(())
}

pub(super) fn print_doctor(
    status: &str,
    prerequisites: &[PrerequisiteReceipt],
    elapsed_ms: u128,
) -> Result<()> {
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "schema_version": 1,
            "command": "doctor",
            "status": status,
            "prerequisites": prerequisites,
            "elapsed_ms": elapsed_ms
        }))?
    );
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(super) fn print_verify_changed(
    status: &str,
    paths: &[String],
    gates: &[GateReceipt],
    selected_gates: &[&str],
    skipped_gates: &[&str],
    prerequisites: &[PrerequisiteReceipt],
    dry_run: bool,
    elapsed_ms: u128,
    failed_gate: Option<&str>,
    error: Option<&str>,
) -> Result<()> {
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "schema_version": 1,
            "command": "verify-changed",
            "status": status,
            "dry_run": dry_run,
            "changed_paths": paths,
            "gates": gates,
            "selected_gates": selected_gates,
            "skipped_gates": skipped_gates,
            "prerequisites": prerequisites,
            "elapsed_ms": elapsed_ms,
            "failed_gate": failed_gate,
            "error": error
        }))?
    );
    Ok(())
}
