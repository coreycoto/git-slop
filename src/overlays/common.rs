use std::collections::HashSet;

use blake2::Blake2bVar;
use blake2::digest::{Update, VariableOutput};

pub(super) const ORGANIZATION_ANALYSIS_STATUS: &str = "experimental";
pub(super) const ORGANIZATION_ANALYSIS_VERSION: u64 = 2;

pub(super) fn round6(value: f64) -> f64 {
    (value * 1_000_000.0).round() / 1_000_000.0
}

pub(super) fn stable_id(kind: &str, parts: &[&str]) -> String {
    // Match the public v1 ID contract: BLAKE2b with a 16-byte digest over every
    // NUL-terminated part (including the kind), then retain 12 hex characters
    // for the public identifier.
    let mut hasher = Blake2bVar::new(16).expect("valid BLAKE2b output size");
    for part in std::iter::once(kind).chain(parts.iter().copied()) {
        hasher.update(part.as_bytes());
        hasher.update(&[0]);
    }
    let mut digest = [0_u8; 16];
    hasher
        .finalize_variable(&mut digest)
        .expect("BLAKE2b output size is fixed");
    format!("{kind}-{}", hex::encode(&digest[..6]))
}

pub(super) fn is_test_path(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    let name = lower.rsplit('/').next().unwrap_or(&lower);
    lower.starts_with("tests/")
        || lower.starts_with("test/")
        || lower.contains("/tests/")
        || lower.contains("/test/")
        || lower.contains("__tests__")
        || name == "tests.rs"
        || name.starts_with("test_")
        || name.contains("_test.")
        || name.contains(".test.")
        || name.contains(".spec.")
}

pub(super) fn is_verification_path(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    let name = lower.rsplit('/').next().unwrap_or(&lower);
    lower.starts_with(".github/workflows/")
        || ((lower.starts_with("scripts/") || lower.contains("/scripts/"))
            && ["test", "check", "validate", "verify", "ci"]
                .iter()
                .any(|marker| name.contains(marker)))
}

pub(super) fn language_common_term(term: &str) -> bool {
    if term.len() <= 2 {
        return true;
    }
    matches!(
        term,
        "arg"
            | "args"
            | "let"
            | "mut"
            | "assert"
            | "run"
            | "byte"
            | "bytes"
            | "if"
            | "else"
            | "for"
            | "while"
            | "match"
            | "return"
            | "self"
            | "this"
            | "true"
            | "false"
            | "none"
            | "null"
            | "some"
            | "result"
            | "string"
            | "str"
            | "path"
            | "format"
            | "from"
            | "into"
            | "impl"
            | "pub"
            | "use"
            | "mod"
            | "fn"
            | "const"
            | "static"
            | "class"
            | "def"
            | "func"
            | "var"
            | "import"
            | "export"
            | "value"
            | "values"
            | "text"
            | "item"
            | "items"
            | "data"
            | "get"
            | "set"
            | "new"
            | "default"
            | "error"
            | "ok"
            | "map"
            | "iter"
            | "type"
            | "name"
            | "object"
            | "array"
            | "only"
            | "other"
            | "when"
            | "then"
            | "file"
            | "key"
            | "json"
            | "schema"
    )
}

pub(super) fn percentile(mut values: Vec<f64>, quantile: f64) -> f64 {
    if values.is_empty() {
        return 1.0;
    }
    values.sort_by(f64::total_cmp);
    let index = ((values.len() as f64 * quantile).ceil() as usize)
        .saturating_sub(1)
        .min(values.len() - 1);
    values[index]
}

pub(super) fn jaccard(left: &[String], right: &[String]) -> f64 {
    let left: HashSet<&str> = left.iter().map(String::as_str).collect();
    let right: HashSet<&str> = right.iter().map(String::as_str).collect();
    if left.is_empty() && right.is_empty() {
        return 1.0;
    }
    let intersection = left.intersection(&right).count();
    let union = left.union(&right).count();
    if union == 0 {
        0.0
    } else {
        intersection as f64 / union as f64
    }
}

pub(super) fn immediate_parent(path: &str) -> String {
    path.rsplit_once('/')
        .map(|(parent, _)| parent.to_string())
        .unwrap_or_else(|| ".".to_string())
}

#[cfg(test)]
mod tests {
    use super::{is_test_path, is_verification_path, stable_id};

    #[test]
    fn ids_match_the_nul_delimited_blake2b_contract() {
        assert_eq!(
            stable_id("temporal_coupling_edge", &["src/a.rs", "src/b.rs"]),
            "temporal_coupling_edge-5948d6886885"
        );
        assert_eq!(
            stable_id("duplicate_neighborhood", &["src/a.rs", "src/b.rs"]),
            "duplicate_neighborhood-2e9ae1bb29e0"
        );
        assert_eq!(
            stable_id("duplicate_set", &["src/a.rs", "src/b.rs"]),
            "duplicate_set-db01f3038ceb"
        );
    }

    #[test]
    fn workflow_and_validation_surfaces_are_not_reported_as_tests() {
        for path in [
            ".github/workflows/contracts.yml",
            "scripts/validate-package.sh",
        ] {
            assert!(is_verification_path(path));
            assert!(!is_test_path(path));
        }
        assert!(is_test_path("tests/contract.rs"));
        assert!(!is_verification_path("tests/contract.rs"));
    }
}
