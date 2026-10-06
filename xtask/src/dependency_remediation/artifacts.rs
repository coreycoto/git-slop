use anyhow::{Context, Result, ensure};
use serde_json::Value;
use std::{env, process::Command};

pub fn verify(id: &str, name: &str, digest: &str) -> Result<()> {
    let id = parse_positive_id(id)?;
    ensure!(!name.is_empty(), "artifact name is required");
    ensure!(valid_digest(digest), "artifact digest is malformed");
    let repository = env::var("GITHUB_REPOSITORY").context("GITHUB_REPOSITORY is required")?;
    let endpoint = format!("repos/{repository}/actions/artifacts/{id}");
    let output = Command::new("gh")
        .args(["api", &endpoint])
        .output()
        .context("gh could not read exact Actions artifact metadata")?;
    ensure!(
        output.status.success(),
        "gh could not read exact Actions artifact metadata"
    );
    let metadata: Value = serde_json::from_slice(&output.stdout)
        .context("Actions artifact metadata was malformed")?;
    validate_metadata(&metadata, id, name, digest)?;
    println!("verified exact Actions artifact {id}: {name}");
    Ok(())
}

pub(super) fn parse_positive_id(value: &str) -> Result<u64> {
    let id = value
        .parse::<u64>()
        .context("artifact ID must be a positive decimal integer")?;
    ensure!(
        id > 0 && id.to_string() == value,
        "artifact ID is not canonical"
    );
    Ok(id)
}

pub(super) fn valid_digest(value: &str) -> bool {
    value.strip_prefix("sha256:").is_some_and(|hex| {
        hex.len() == 64
            && hex
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    })
}

fn validate_metadata(metadata: &Value, id: u64, name: &str, digest: &str) -> Result<()> {
    ensure!(
        metadata.get("id").and_then(Value::as_u64) == Some(id)
            && metadata.get("name").and_then(Value::as_str) == Some(name)
            && metadata.get("digest").and_then(Value::as_str) == Some(digest)
            && metadata.get("expired").and_then(Value::as_bool) == Some(false),
        "Actions artifact metadata differs from the exact upstream identity"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{parse_positive_id, valid_digest, validate_metadata};
    use serde_json::json;

    const DIGEST: &str = "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

    #[test]
    fn artifact_metadata_requires_exact_numeric_id_name_digest_and_live_state() {
        let expected = json!({"id":123,"name":"capture-7-1","digest":DIGEST,"expired":false});
        assert!(validate_metadata(&expected, 123, "capture-7-1", DIGEST).is_ok());
        for metadata in [
            json!({"id":"123","name":"capture-7-1","digest":DIGEST,"expired":false}),
            json!({"id":123,"name":"other","digest":DIGEST,"expired":false}),
            json!({"id":123,"name":"capture-7-1","digest":"sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb","expired":false}),
            json!({"id":123,"name":"capture-7-1","digest":DIGEST,"expired":true}),
        ] {
            assert!(validate_metadata(&metadata, 123, "capture-7-1", DIGEST).is_err());
        }
    }

    #[test]
    fn artifact_inputs_are_canonical_and_sha256_formatted() {
        assert_eq!(parse_positive_id("123").unwrap(), 123);
        for invalid in ["0", "0123", "+123", " 123", "123x"] {
            assert!(parse_positive_id(invalid).is_err());
        }
        assert!(valid_digest(DIGEST));
        assert!(!valid_digest(
            "sha256:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"
        ));
        assert!(!valid_digest(
            "sha512:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
        ));
    }
}
