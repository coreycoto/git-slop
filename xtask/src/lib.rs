pub mod codex;
pub mod crates_io;
pub mod distribution;
pub mod homebrew;
pub mod issue_forms;
pub mod manifest;
pub mod release;
pub mod repository;
pub mod workflows;

use anyhow::{Result, bail};

pub fn finish_validation(label: &str, errors: Vec<String>) -> Result<()> {
    if errors.is_empty() {
        println!("{label} validation passed.");
        return Ok(());
    }

    for error in &errors {
        eprintln!("{error}");
    }
    bail!("{label} validation failed with {} error(s)", errors.len())
}
