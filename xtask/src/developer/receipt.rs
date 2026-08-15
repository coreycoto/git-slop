use anyhow::Result;
use serde_json::json;

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

pub(super) fn print_failure(
    passed_gates: &[&str],
    failed_gate: &str,
    elapsed_ms: u128,
    error: &str,
) -> Result<()> {
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "schema_version": 1,
            "command": "ci",
            "status": "failed",
            "passed_gates": passed_gates,
            "failed_gate": failed_gate,
            "elapsed_ms": elapsed_ms,
            "error": error
        }))?
    );
    Ok(())
}

pub(super) fn print_success(passed_gates: &[&str], elapsed_ms: u128) -> Result<()> {
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "schema_version": 1,
            "command": "ci",
            "status": "passed",
            "passed_gates": passed_gates,
            "failed_gate": null,
            "elapsed_ms": elapsed_ms
        }))?
    );
    Ok(())
}
