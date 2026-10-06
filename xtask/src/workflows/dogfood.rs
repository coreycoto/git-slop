use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use serde_json::Value;

const MANIFEST: &str = "config/github/dogfood-regression-acceptances.json";
const SHARDS: &str = "config/github/dogfood-regression-acceptances";

pub(super) fn validate_acceptance_manifests(repo_root: &Path, errors: &mut Vec<String>) {
    let manifest = repo_root.join(MANIFEST);
    let mut inputs = vec![(manifest, None)];
    let shard_dir = repo_root.join(SHARDS);
    match fs::symlink_metadata(&shard_dir) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => {
            errors.push(format!("{SHARDS} must be a real directory when present."));
            return;
        }
        Ok(_) => match fs::read_dir(&shard_dir) {
            Ok(entries) => {
                for entry in entries {
                    let Ok(entry) = entry else {
                        errors.push(format!("{SHARDS} could not be read completely."));
                        continue;
                    };
                    let path = entry.path();
                    let file_type = match entry.file_type() {
                        Ok(file_type) => file_type,
                        Err(error) => {
                            errors.push(format!(
                                "{} could not be inspected: {error}",
                                path.display()
                            ));
                            continue;
                        }
                    };
                    let Some(base) = shard_base(&path) else {
                        errors.push(format!(
                            "{} is not a canonical SHA-named dogfood shard.",
                            path.display()
                        ));
                        continue;
                    };
                    if file_type.is_symlink() || !file_type.is_file() {
                        errors.push(format!(
                            "{} must be a regular dogfood shard file.",
                            path.display()
                        ));
                        continue;
                    }
                    inputs.push((path, Some(base)));
                }
            }
            Err(error) => errors.push(format!("{SHARDS} could not be read: {error}")),
        },
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => errors.push(format!("{SHARDS} could not be inspected: {error}")),
    }

    let mut base_shas = BTreeSet::new();
    let mut total_entries = 0usize;
    for (path, expected_base) in inputs {
        let relative = path
            .strip_prefix(repo_root)
            .unwrap_or(&path)
            .display()
            .to_string();
        match fs::symlink_metadata(&path) {
            Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => {
                errors.push(format!("{relative} must be a regular manifest file."));
                continue;
            }
            Ok(_) => {}
            Err(error) => {
                errors.push(format!("{relative} could not be inspected: {error}"));
                continue;
            }
        }
        let text = match fs::read_to_string(&path) {
            Ok(text) => text,
            Err(error) => {
                errors.push(format!("{relative} could not be read: {error}"));
                continue;
            }
        };
        let document: Value = match serde_json::from_str(&text) {
            Ok(document) => document,
            Err(error) => {
                errors.push(format!("{relative} is not valid JSON: {error}"));
                continue;
            }
        };
        if validate_document(
            &document,
            expected_base.as_deref(),
            &mut base_shas,
            &mut total_entries,
        ) {
            errors.push(format!(
                "{relative} violates the dogfood acceptance schema."
            ));
        }
    }
    if total_entries == 0 {
        errors.push(format!(
            "{MANIFEST} and its shards must contain reviewed entries."
        ));
    }
}

fn shard_base(path: &Path) -> Option<String> {
    let name = path.file_name()?.to_str()?;
    let base = name.strip_suffix(".json")?;
    is_lower_hex(base, 40).then(|| base.to_string())
}

fn validate_document(
    document: &Value,
    expected_base: Option<&str>,
    seen_bases: &mut BTreeSet<String>,
    total_entries: &mut usize,
) -> bool {
    if document.get("schema_version") != Some(&Value::from(1)) {
        return true;
    }
    let Some(acceptances) = document.get("acceptances").and_then(Value::as_array) else {
        return true;
    };
    if let Some(expected_base) = expected_base {
        if acceptances.len() != 1
            || acceptances[0].get("base_sha").and_then(Value::as_str) != Some(expected_base)
        {
            return true;
        }
    }

    let mut invalid = false;
    for acceptance in acceptances {
        let Some(base_sha) = acceptance.get("base_sha").and_then(Value::as_str) else {
            invalid = true;
            continue;
        };
        if !is_lower_hex(base_sha, 40) || !seen_bases.insert(base_sha.to_string()) {
            invalid = true;
        }
        let rationale_valid = acceptance
            .get("rationale")
            .and_then(Value::as_str)
            .is_some_and(|value| !value.is_empty() && value.chars().count() <= 500);
        let Some(entries) = acceptance.get("entries").and_then(Value::as_array) else {
            invalid = true;
            continue;
        };
        if !rationale_valid || entries.is_empty() {
            invalid = true;
        }
        let mut paths = BTreeSet::new();
        for entry in entries {
            *total_entries += 1;
            let Some(path) = entry.get("path").and_then(Value::as_str) else {
                invalid = true;
                continue;
            };
            if !valid_path(path) || !paths.insert(path) {
                invalid = true;
            }
            if !matches!(
                entry.get("reason").and_then(Value::as_str),
                Some("material_score_increase" | "worse_band" | "new_finding")
            ) || !matches!(
                entry.get("severity").and_then(Value::as_str),
                Some("notice" | "warning")
            ) || !entry
                .get("content_sha256")
                .and_then(Value::as_str)
                .is_some_and(|digest| is_lower_hex(digest, 64))
                || !entry
                    .get("maximum_slop_score")
                    .and_then(Value::as_f64)
                    .is_some_and(|score| (0.0..=100.0).contains(&score))
            {
                invalid = true;
            }
        }
    }
    invalid
}

fn valid_path(path: &str) -> bool {
    !path.is_empty()
        && path.chars().count() <= 512
        && !path.starts_with('/')
        && !path.chars().any(char::is_control)
        && path
            .split('/')
            .all(|part| !part.is_empty() && part != "." && part != "..")
}

fn is_lower_hex(value: &str, length: usize) -> bool {
    value.len() == length
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}
