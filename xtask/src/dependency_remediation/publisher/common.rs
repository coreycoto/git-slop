use anyhow::{Context, Result, ensure};
use serde_json::Value;
use std::{collections::BTreeSet, env, fs, io::Write, path::Path, process::Command};

pub(super) fn write_json_new_or_replace(path: &Path, value: &Value) -> Result<()> {
    if let Some(parent) = path.parent() {
        ensure_real_dir(parent)?;
    }
    let temp = path.with_extension(format!("tmp-{}", std::process::id()));
    let bytes = serde_json::to_vec(value)?;
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options
        .open(&temp)
        .context("could not create durable dependency publication receipt")?;
    file.write_all(&bytes)?;
    file.sync_all()?;
    fs::rename(&temp, path).context("could not atomically write dependency publication receipt")?;
    Ok(())
}

pub(super) fn ensure_real_dir(path: &Path) -> Result<()> {
    if !path.exists() {
        fs::create_dir_all(path)?;
    }
    let metadata = fs::symlink_metadata(path)?;
    ensure!(
        metadata.is_dir() && !metadata.file_type().is_symlink(),
        "publication package path is not a real directory"
    );
    Ok(())
}
pub(super) fn require_regular(path: &Path, message: &str) -> Result<()> {
    let metadata = fs::symlink_metadata(path).with_context(|| message.to_owned())?;
    ensure!(
        metadata.is_file() && !metadata.file_type().is_symlink(),
        "{message}"
    );
    Ok(())
}
pub(super) fn read_json(path: &Path) -> Result<Value> {
    require_regular(path, "required JSON document is missing or unsafe")?;
    Ok(serde_json::from_slice(&fs::read(path)?)?)
}
pub(super) fn sha256(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    hex::encode(Sha256::digest(bytes))
}
pub(super) fn validate_sha40(value: &str) -> Result<()> {
    ensure!(
        value.len() == 40
            && value
                .bytes()
                .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()),
        "saved commit identity is malformed"
    );
    Ok(())
}
pub(super) fn is_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
}
pub(super) fn is_sha256_artifact(value: &str) -> bool {
    value.strip_prefix("sha256:").is_some_and(is_sha256)
}
pub(super) fn positive_integer(value: &str) -> Result<u64> {
    let parsed = value
        .parse::<u64>()
        .context("value must be a positive integer")?;
    ensure!(
        parsed > 0 && parsed.to_string() == value,
        "value must be a positive integer"
    );
    Ok(parsed)
}
pub(super) fn positive_u64(value: &Value) -> Result<u64> {
    let parsed = value.as_u64().context("value must be a positive integer")?;
    ensure!(parsed > 0, "value must be a positive integer");
    Ok(parsed)
}
pub(super) fn required_env(name: &str) -> Result<String> {
    env::var(name).with_context(|| format!("{name} is required"))
}
pub(super) fn require_env(name: &str, error: &str) -> Result<String> {
    env::var(name).map_err(|_| anyhow::anyhow!("{error}"))
}
pub(super) fn value_string<'a>(value: &'a Value, key: &str) -> Result<&'a str> {
    value
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("{key} must be a string"))
}
pub(super) fn string_at<'a>(value: &'a Value, path: &[&str]) -> Result<&'a str> {
    let mut item = value;
    for key in path {
        item = item
            .get(*key)
            .ok_or_else(|| anyhow::anyhow!("required field {} is missing", path.join(".")))?;
    }
    item.as_str()
        .ok_or_else(|| anyhow::anyhow!("{} must be a string", path.join(".")))
}
pub(super) fn integer_at(value: &Value, path: &[&str]) -> Result<u64> {
    let mut item = value;
    for key in path {
        item = item
            .get(*key)
            .ok_or_else(|| anyhow::anyhow!("required field {} is missing", path.join(".")))?;
    }
    positive_u64(item)
}
pub(super) fn exact_keys(value: &Value, expected: &[&str]) -> bool {
    value.as_object().is_some_and(|object| {
        let actual: BTreeSet<_> = object.keys().map(String::as_str).collect();
        let expected: BTreeSet<_> = expected.iter().copied().collect();
        actual == expected
    })
}
pub(super) fn path_text(path: &Path) -> Result<&str> {
    path.to_str()
        .ok_or_else(|| anyhow::anyhow!("package path must be valid UTF-8"))
}
pub(super) fn uuid_nonce() -> Result<String> {
    let output = Command::new("uuidgen")
        .output()
        .context("could not create an exact publication nonce")?;
    ensure!(
        output.status.success(),
        "could not create an exact publication nonce"
    );
    let value = String::from_utf8(output.stdout)?
        .trim()
        .to_ascii_lowercase();
    let bytes = value.as_bytes();
    ensure!(
        bytes.len() == 36
            && [8, 13, 18, 23].iter().all(|index| bytes[*index] == b'-')
            && bytes
                .iter()
                .enumerate()
                .all(|(i, b)| [8, 13, 18, 23].contains(&i) || b.is_ascii_hexdigit())
            && (b'1'..=b'5').contains(&bytes[14])
            && matches!(bytes[19], b'8' | b'9' | b'a' | b'b'),
        "could not create an exact publication nonce"
    );
    Ok(value)
}
