use std::fs::File;
use std::io::Read;
use std::path::Path;

use anyhow::{Context, Result, bail};

pub(super) const MAX_ADVICE_ARTIFACT_BYTES: usize = 8 * 1024 * 1024;
pub(super) const MAX_ADVICE_REPORT_BYTES: usize = 256 * 1024 * 1024;
pub(super) const MAX_ADVICE_CONTEXT_CACHE_BYTES: usize = 2 * 1024 * 1024;
pub(super) const MAX_MOCK_RESPONSE_BYTES: usize = 4 * 1024 * 1024;

pub(super) fn read_bounded(path: &Path, maximum: usize, label: &str) -> Result<Vec<u8>> {
    let metadata = path
        .metadata()
        .with_context(|| format!("unable to inspect {label} {}", path.display()))?;
    if metadata.len() > maximum as u64 {
        bail!(
            "advisor_input_too_large: {label} {} is {} bytes; maximum is {maximum} bytes",
            path.display(),
            metadata.len()
        );
    }
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    File::open(path)
        .with_context(|| format!("unable to open {label} {}", path.display()))?
        .take((maximum as u64).saturating_add(1))
        .read_to_end(&mut bytes)
        .with_context(|| format!("unable to read {label} {}", path.display()))?;
    if bytes.len() > maximum {
        bail!(
            "advisor_input_too_large: {label} {} exceeds {maximum} bytes",
            path.display()
        );
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounded_reader_accepts_exact_limit_and_rejects_larger_inputs() {
        let temporary = tempfile::tempdir().expect("temporary directory");
        let path = temporary.path().join("input.json");
        std::fs::write(&path, b"1234").expect("bounded fixture");
        assert_eq!(read_bounded(&path, 4, "test input").unwrap(), b"1234");
        std::fs::write(&path, b"12345").expect("oversized fixture");
        let error = read_bounded(&path, 4, "test input").expect_err("oversized input");
        assert!(error.to_string().contains("advisor_input_too_large"));
    }
}
