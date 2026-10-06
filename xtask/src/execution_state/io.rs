use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Component, Path, PathBuf},
};

use anyhow::{Context, Result, bail, ensure};
use serde_json::Value;

pub(super) fn inputs(event: &Value) -> Result<&Value> {
    event
        .get("inputs")
        .context("raw workflow_dispatch event has no inputs object")
}

pub(super) fn optional_string(value: &Value, key: &str) -> Result<String> {
    match value.get(key) {
        None | Some(Value::Null) => Ok(String::new()),
        Some(Value::String(value)) => Ok(value.clone()),
        _ => bail!("workflow_dispatch input {key} is not a string"),
    }
}

pub(super) fn string_field(value: &Value, key: &str) -> Result<String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(str::to_owned)
        .with_context(|| format!("JSON field {key} is missing or not a string"))
}

pub(super) fn field<'a>(value: &'a Value, key: &str) -> Result<&'a Value> {
    value
        .get(key)
        .with_context(|| format!("JSON field {key} is missing"))
}

pub(super) fn positive_number(value: &Value, label: &str) -> Result<u64> {
    value
        .as_u64()
        .filter(|number| *number > 0)
        .with_context(|| format!("{label} is not a positive integer"))
}

pub(super) fn json_bytes(value: &Value) -> Result<Vec<u8>> {
    serde_json::to_vec(value).context("serialize execution-state adapter JSON")
}

pub(super) fn parse_json(bytes: &[u8], label: &str) -> Result<Value> {
    serde_json::from_slice(bytes).with_context(|| format!("parse {label} JSON"))
}

pub(super) fn read_json(path: &Path) -> Result<Value> {
    let bytes = fs::read(path).with_context(|| format!("read {}", path.display()))?;
    parse_json(&bytes, &path.display().to_string())
}

pub(super) fn read_package_json(root: &Path, relative: &str) -> Result<Value> {
    let bytes = read_package_file(root, relative)?;
    parse_json(&bytes, relative)
}

pub(super) fn read_package_file(root: &Path, relative: &str) -> Result<Vec<u8>> {
    let path = package_file(root, relative)?;
    fs::read(&path).with_context(|| format!("read retained package file {relative}"))
}

pub(super) fn package_file(root: &Path, relative: &str) -> Result<PathBuf> {
    let rel = Path::new(relative);
    ensure!(
        rel.components()
            .all(|component| matches!(component, Component::Normal(_))),
        "package path is unsafe"
    );
    let path = root.join(rel);
    let metadata = fs::symlink_metadata(&path)
        .with_context(|| format!("inspect retained package file {relative}"))?;
    ensure!(
        metadata.file_type().is_file(),
        "retained package file {relative} is not a regular file"
    );
    Ok(path)
}

pub(super) fn read_json_documents(path: &Path) -> Result<Vec<Value>> {
    let bytes = fs::read(path).with_context(|| format!("read {}", path.display()))?;
    let stream = serde_json::Deserializer::from_slice(&bytes).into_iter::<Value>();
    let mut documents = Vec::new();
    for document in stream {
        documents.push(
            document
                .with_context(|| format!("parse GitHub API response stream {}", path.display()))?,
        );
    }
    ensure!(
        !documents.is_empty(),
        "GitHub artifact API response stream is empty"
    );
    Ok(documents)
}

pub(super) fn ensure_new(path: &Path) -> Result<()> {
    ensure!(
        fs::symlink_metadata(path).is_err_and(|error| error.kind() == std::io::ErrorKind::NotFound),
        "refusing to replace existing execution-state evidence at {}",
        path.display()
    );
    Ok(())
}

pub(super) fn create_private_dir(path: PathBuf) -> Result<()> {
    fs::create_dir_all(&path).with_context(|| format!("create {}", path.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o700))
            .with_context(|| format!("restrict permissions on {}", path.display()))?;
    }
    Ok(())
}

pub(super) fn write_new(path: &Path, bytes: &[u8]) -> Result<()> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options
        .open(path)
        .with_context(|| format!("create {} without replacement", path.display()))?;
    file.write_all(bytes)
        .with_context(|| format!("write {}", path.display()))?;
    file.sync_all()
        .with_context(|| format!("sync {}", path.display()))?;
    Ok(())
}

pub(super) fn append_outputs(path: &Path, outputs: &[(&str, String)]) -> Result<()> {
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .with_context(|| format!("open GitHub output {}", path.display()))?;
    for (key, value) in outputs {
        ensure!(
            !value.contains(['\n', '\r']),
            "GitHub output {key} contains a line break"
        );
        writeln!(file, "{key}={value}").with_context(|| format!("write GitHub output {key}"))?;
    }
    file.flush().context("flush GitHub outputs")?;
    Ok(())
}
