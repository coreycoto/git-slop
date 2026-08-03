use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde_json::{Value, json};

pub const DEFAULT_SLOP_GITIGNORE: &str = "/latest/\n/runs/\n/cache/\n";

#[derive(Debug, Clone)]
pub struct InitResult {
    pub config: &'static str,
    pub gitignore: &'static str,
}

pub fn slop_dir(repo_root: &Path) -> PathBuf {
    repo_root.join(".slop")
}

pub fn config_path(repo_root: &Path) -> PathBuf {
    slop_dir(repo_root).join("config.yaml")
}

pub fn latest_dir(repo_root: &Path) -> PathBuf {
    slop_dir(repo_root).join("latest")
}

pub fn runs_dir(repo_root: &Path) -> PathBuf {
    slop_dir(repo_root).join("runs")
}

pub fn cache_dir(repo_root: &Path) -> PathBuf {
    slop_dir(repo_root).join("cache")
}

pub fn default_config() -> Value {
    json!({
        "schema_version": 2,
        "inventory": {
            "ignore_globs": [
                "uv.lock", "poetry.lock", "Pipfile.lock", "package-lock.json",
                "pnpm-lock.yaml", "yarn.lock", "bun.lock", "bun.lockb",
                "Cargo.lock", "Gemfile.lock", "composer.lock", "Podfile.lock"
            ]
        },
        "tokenization": {
            "context_tokenizer_name": "cl100k_base",
            "context_bands": {
                "compact_max_tokens": 3072,
                "healthy_max_tokens": 8000,
                "warning_max_tokens": 10000
            }
        },
        "history": {
            "churn_window_days": 180,
            "age_half_life_days": 180,
            "follow_renames": false
        },
        "scoring": {
            "context_weight": 0.60,
            "age_weight": 0.20,
            "churn_weight": 0.20
        },
        "organization": {
            "candidate_file_limit": 500,
            "min_file_tokens": 300,
            "max_file_tokens": 50000,
            "shingle_size": 8,
            "window_step": 32,
            "min_similarity": 0.72,
            "max_pairs_per_file": 20,
            "min_cochange_support": 3,
            "min_coupling_lift": 2.0
        },
        "verification": {
            "test_path_markers": [
                "test/", "tests/", "spec/", "__tests__/", ".test.", ".spec."
            ]
        },
        "navigation": {"top_distinctive_terms": 5},
        "blast_radius": {},
        "stewardship": {"bot_name_markers": ["bot", "[bot]"]},
        "semantic_drift": {"top_term_limit": 25},
        "health": {
            "data_context_min_bytes": 262144,
            "folder_bands": {
                "compact_max_direct_tokens": 31999,
                "healthy_max_direct_tokens": 128000,
                "warning_max_direct_tokens": 256000,
                "warning_max_direct_files": 17,
                "refactor_required_max_direct_files": 37
            },
            "summary_top_files": 10,
            "summary_top_folders": 10
        },
        "check": {
            "fail_on_context_band": "critical",
            "fail_on_slop_band": "critical"
        }
    })
}

fn deep_merge(base: &mut Value, override_value: Value) {
    match (base, override_value) {
        (Value::Object(base_map), Value::Object(override_map)) => {
            for (key, value) in override_map {
                if let Some(existing) = base_map.get_mut(&key) {
                    deep_merge(existing, value);
                } else {
                    base_map.insert(key, value);
                }
            }
        }
        (base_slot, value) => *base_slot = value,
    }
}

fn normalize_legacy(mut payload: Value) -> Result<Value> {
    let Some(object) = payload.as_object_mut() else {
        bail!("config.yaml must decode to a mapping.");
    };
    let schema = object
        .get("schema_version")
        .and_then(Value::as_u64)
        .unwrap_or(1);
    if schema != 1 && schema != 2 {
        bail!("config.yaml must declare schema_version: 1 or schema_version: 2.");
    }
    if schema == 1 {
        object.insert("schema_version".into(), json!(2));
        let tokenizer_name = object
            .remove("tokenizer")
            .and_then(|item| item.get("name").cloned());
        let legacy_bands = object.remove("context_bands");
        if tokenizer_name.is_some() || legacy_bands.is_some() {
            let tokenization = object.entry("tokenization").or_insert_with(|| json!({}));
            let Some(tokenization) = tokenization.as_object_mut() else {
                bail!("tokenization must be a mapping.");
            };
            if let Some(name) = tokenizer_name {
                tokenization.entry("context_tokenizer_name").or_insert(name);
            }
            if let Some(bands) = legacy_bands {
                tokenization.entry("context_bands").or_insert(bands);
            }
        }
    }
    if let Some(check) = object.get_mut("check").and_then(Value::as_object_mut) {
        if !check.contains_key("fail_on_slop_band") {
            if let Some(legacy) = check.remove("fail_on_priority_band") {
                let mapped = match legacy.as_str().unwrap_or_default() {
                    "watchlist" => "low",
                    "needs_refactor" => "moderate",
                    "should_refactor" => "high",
                    "must_refactor" => "critical",
                    other => other,
                };
                check.insert("fail_on_slop_band".into(), json!(mapped));
            }
        }
    }
    Ok(payload)
}

fn add_legacy_aliases(mut payload: Value) -> Value {
    let tokenizer_name = pointer_str(&payload, "/tokenization/context_tokenizer_name")
        .unwrap_or("cl100k_base")
        .to_string();
    let bands = payload
        .pointer("/tokenization/context_bands")
        .cloned()
        .unwrap_or_else(|| json!({}));
    if let Some(object) = payload.as_object_mut() {
        object.insert("tokenizer".into(), json!({"name": tokenizer_name}));
        object.insert("context_bands".into(), bands);
    }
    payload
}

pub fn load(repo_root: &Path) -> Result<Value> {
    let path = config_path(repo_root);
    if !path.exists() {
        return Ok(add_legacy_aliases(default_config()));
    }
    let source =
        fs::read_to_string(&path).with_context(|| format!("failed to read {}", path.display()))?;
    let yaml: serde_yaml::Value =
        serde_yaml::from_str(&source).with_context(|| format!("invalid {}", path.display()))?;
    let override_value =
        serde_json::to_value(yaml).context("config.yaml contains unsupported YAML values")?;
    let override_value = normalize_legacy(override_value)?;
    let mut merged = default_config();
    deep_merge(&mut merged, override_value);
    Ok(add_legacy_aliases(merged))
}

pub fn ensure_state_dirs(repo_root: &Path) -> Result<()> {
    for path in [
        slop_dir(repo_root),
        latest_dir(repo_root),
        runs_dir(repo_root),
        cache_dir(repo_root),
    ] {
        fs::create_dir_all(&path)
            .with_context(|| format!("failed to create {}", path.display()))?;
    }
    Ok(())
}

pub fn initialize(repo_root: &Path, force: bool) -> Result<InitResult> {
    ensure_state_dirs(repo_root)?;
    let config_target = config_path(repo_root);
    let config_status = if force || !config_target.exists() {
        let yaml = serde_yaml::to_string(&default_config())?;
        fs::write(&config_target, yaml)
            .with_context(|| format!("failed to write {}", config_target.display()))?;
        "written"
    } else {
        "kept"
    };
    let gitignore_target = slop_dir(repo_root).join(".gitignore");
    let gitignore_status = if force || !gitignore_target.exists() {
        fs::write(&gitignore_target, DEFAULT_SLOP_GITIGNORE)
            .with_context(|| format!("failed to write {}", gitignore_target.display()))?;
        "written"
    } else {
        "kept"
    };
    Ok(InitResult {
        config: config_status,
        gitignore: gitignore_status,
    })
}

pub fn pointer_u64(value: &Value, pointer: &str, default: u64) -> u64 {
    value
        .pointer(pointer)
        .and_then(Value::as_u64)
        .unwrap_or(default)
}

pub fn pointer_f64(value: &Value, pointer: &str, default: f64) -> f64 {
    value
        .pointer(pointer)
        .and_then(Value::as_f64)
        .unwrap_or(default)
}

pub fn pointer_bool(value: &Value, pointer: &str, default: bool) -> bool {
    value
        .pointer(pointer)
        .and_then(Value::as_bool)
        .unwrap_or(default)
}

pub fn pointer_str<'a>(value: &'a Value, pointer: &str) -> Option<&'a str> {
    value.pointer(pointer).and_then(Value::as_str)
}

pub fn pointer_strings(value: &Value, pointer: &str) -> Vec<String> {
    value
        .pointer(pointer)
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(ToOwned::to_owned)
        .collect()
}
