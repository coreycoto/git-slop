use anyhow::{Result, bail};
use serde_json::Value;
use std::{fs, path::Path};

pub fn path_allowed(path: &str) -> bool {
    if path.is_empty()
        || path.starts_with('/')
        || path.ends_with('/')
        || path.contains('\\')
        || path.contains("//")
        || path.contains('\0')
        || path
            .split('/')
            .any(|component| component.is_empty() || component == "." || component == "..")
    {
        return false;
    }

    let nested_scope = ["action/", "tools/", "src/", "tests/"]
        .iter()
        .any(|prefix| path.starts_with(prefix));
    let top_level_scope = ["src/", "tests/", "action/", "tools/"]
        .iter()
        .any(|prefix| path.starts_with(prefix));
    if top_level_scope {
        return true;
    }

    let file_name = path.rsplit('/').next().unwrap_or(path);
    if path.contains('/') {
        return nested_scope && is_manifest_name(file_name);
    }

    is_manifest_name(path)
        || matches!(
            path,
            "rust-toolchain.toml"
                | "package.json"
                | "package-lock.json"
                | "pnpm-lock.yaml"
                | "yarn.lock"
                | "bun.lock"
                | "bun.lockb"
                | "go.mod"
                | "go.sum"
                | "pyproject.toml"
                | "uv.lock"
                | "requirements.txt"
                | "Gemfile"
                | "Gemfile.lock"
        )
        || path.starts_with("requirements-") && path.ends_with(".txt")
}

fn is_manifest_name(name: &str) -> bool {
    matches!(
        name,
        "Cargo.toml"
            | "Cargo.lock"
            | "package.json"
            | "package-lock.json"
            | "pnpm-lock.yaml"
            | "yarn.lock"
            | "bun.lock"
            | "bun.lockb"
            | "go.mod"
            | "go.sum"
            | "pyproject.toml"
            | "uv.lock"
            | "requirements.txt"
            | "Gemfile"
            | "Gemfile.lock"
    ) || name.starts_with("requirements-") && name.ends_with(".txt")
}

pub fn read_result(path: &Path) -> Result<Value> {
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        bail!("dependency-remediation result must be a regular file");
    }
    let bytes = fs::read(path)?;
    let value: Value = serde_json::from_slice(&bytes)?;
    let object = value
        .as_object()
        .ok_or_else(|| anyhow::anyhow!("Codex result must be a JSON object"))?;
    if object.get("status").and_then(Value::as_str) != Some("patched")
        || !nonempty_string(object.get("summary"))
        || !bounded_string(object.get("title"), 256)
        || !bounded_string(object.get("body"), 20_000)
        || !nonempty_string(object.get("supplemental_patch"))
        || !object.get("draft").is_some_and(Value::is_boolean)
        || !object
            .get("verification")
            .and_then(Value::as_array)
            .is_some_and(|items| {
                !items.is_empty() && items.iter().all(|value| nonempty_string(Some(value)))
            })
    {
        bail!("Codex result is not a verified patched result");
    }
    validate_result_paths(&value)?;
    Ok(value)
}

pub fn validate_result_paths(value: &Value) -> Result<Vec<String>> {
    let files = value
        .get("changed_files")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow::anyhow!("changed_files must be an array"))?;
    if files.is_empty() {
        bail!("changed_files must contain at least one path");
    }
    let mut unique = std::collections::BTreeSet::new();
    let mut paths = Vec::with_capacity(files.len());
    for item in files {
        let path = item
            .as_str()
            .filter(|value| !value.is_empty())
            .ok_or_else(|| anyhow::anyhow!("changed_files entries must be nonempty strings"))?;
        if !unique.insert(path) {
            bail!("changed_files must not contain duplicates");
        }
        if !path_allowed(path) {
            bail!("changed file is unsafe or outside dependency-remediation scope: {path:?}");
        }
        paths.push(path.to_owned());
    }
    Ok(paths)
}

fn nonempty_string(value: Option<&Value>) -> bool {
    value
        .and_then(Value::as_str)
        .is_some_and(|text| !text.is_empty())
}

fn bounded_string(value: Option<&Value>, maximum: usize) -> bool {
    value
        .and_then(Value::as_str)
        .is_some_and(|text| !text.is_empty() && text.chars().count() <= maximum)
}
