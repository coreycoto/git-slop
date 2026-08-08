use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde_json::{Value, json};

pub const DEFAULT_SLOP_GITIGNORE: &str = "/latest/\n/runs/\n/cache/\n";
pub const MINIMAL_CONFIG: &str = r#"# Git Slop configuration overrides.
# Run `git slop config show --effective` to inspect every default.
schema_version: 2

# Example:
# check:
#   fail_on_context_band: critical
#   fail_on_slop_band: critical
"#;

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
            "max_commits": 10000,
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
            "max_temporal_edges": 10000,
            "max_commit_files": 200,
            "min_cochange_support": 3,
            "min_coupling_lift": 2.0
        },
        "verification": {
            "test_path_markers": [
                "test/", "tests/", "spec/", "__tests__/", ".test.", ".spec."
            ],
            "source_test_mappings": []
        },
        "navigation": {"top_distinctive_terms": 5},
        "blast_radius": {},
        "stewardship": {"bot_name_markers": ["bot", "[bot]"]},
        "semantic_drift": {"top_term_limit": 25},
        "resources": {
            "memory_budget_mb": 1024,
            "large_file_bytes": 2097152
        },
        "output": {
            "retention_runs": 20
        },
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

fn validate_override_shape(value: &Value, defaults: &Value, path: &str) -> Result<()> {
    match (value, defaults) {
        (Value::Object(values), Value::Object(defaults)) => {
            for (key, child) in values {
                let child_path = if path.is_empty() {
                    key.to_string()
                } else {
                    format!("{path}.{key}")
                };
                let Some(default) = defaults.get(key) else {
                    bail!("unknown configuration key {child_path}");
                };
                validate_override_shape(child, default, &child_path)?;
            }
        }
        (Value::Array(values), Value::Array(defaults)) => {
            if path == "verification.source_test_mappings" {
                for (index, mapping) in values.iter().enumerate() {
                    let Some(mapping) = mapping.as_object() else {
                        bail!("{path}[{index}] must be a mapping");
                    };
                    if mapping
                        .keys()
                        .any(|key| key != "source_glob" && key != "test_glob")
                    {
                        bail!("{path}[{index}] supports only source_glob and test_glob");
                    }
                    for key in ["source_glob", "test_glob"] {
                        if mapping.get(key).and_then(Value::as_str).is_none() {
                            bail!("{path}[{index}].{key} must be a string");
                        }
                    }
                }
            } else if let Some(default) = defaults.first() {
                for (index, child) in values.iter().enumerate() {
                    validate_override_shape(child, default, &format!("{path}[{index}]"))?;
                }
            } else if !values.is_empty() {
                bail!("{path} does not accept configured entries");
            }
        }
        (Value::Number(_), Value::Number(_))
        | (Value::String(_), Value::String(_))
        | (Value::Bool(_), Value::Bool(_))
        | (Value::Null, Value::Null) => {}
        _ => bail!("configuration key {path} has the wrong type"),
    }
    Ok(())
}

fn require_positive(config: &Value, pointer: &str) -> Result<()> {
    if config.pointer(pointer).and_then(Value::as_u64).unwrap_or(0) == 0 {
        bail!(
            "{} must be a positive integer",
            pointer.trim_start_matches('/').replace('/', ".")
        );
    }
    Ok(())
}

pub fn validate(config: &Value) -> Result<()> {
    for pointer in [
        "/tokenization/context_bands/compact_max_tokens",
        "/tokenization/context_bands/healthy_max_tokens",
        "/tokenization/context_bands/warning_max_tokens",
        "/history/churn_window_days",
        "/history/age_half_life_days",
        "/history/max_commits",
        "/organization/candidate_file_limit",
        "/organization/min_file_tokens",
        "/organization/max_file_tokens",
        "/organization/shingle_size",
        "/organization/window_step",
        "/organization/max_pairs_per_file",
        "/organization/max_temporal_edges",
        "/organization/max_commit_files",
        "/navigation/top_distinctive_terms",
        "/semantic_drift/top_term_limit",
        "/resources/memory_budget_mb",
        "/resources/large_file_bytes",
        "/output/retention_runs",
    ] {
        require_positive(config, pointer)?;
    }
    let bands = &config["tokenization"]["context_bands"];
    let compact = bands["compact_max_tokens"].as_u64().unwrap_or_default();
    let healthy = bands["healthy_max_tokens"].as_u64().unwrap_or_default();
    let warning = bands["warning_max_tokens"].as_u64().unwrap_or_default();
    if !(compact < healthy && healthy < warning) {
        bail!("tokenization.context_bands must be strictly increasing");
    }
    let min_tokens = config["organization"]["min_file_tokens"]
        .as_u64()
        .unwrap_or_default();
    let max_tokens = config["organization"]["max_file_tokens"]
        .as_u64()
        .unwrap_or_default();
    if min_tokens > max_tokens {
        bail!("organization.min_file_tokens must not exceed max_file_tokens");
    }
    for pointer in [
        "/organization/min_similarity",
        "/organization/min_coupling_lift",
    ] {
        let Some(value) = config.pointer(pointer).and_then(Value::as_f64) else {
            bail!(
                "{} must be a number",
                pointer.trim_start_matches('/').replace('/', ".")
            );
        };
        if !value.is_finite() || value < 0.0 {
            bail!(
                "{} must be finite and non-negative",
                pointer.trim_start_matches('/').replace('/', ".")
            );
        }
    }
    let weights = ["context_weight", "age_weight", "churn_weight"]
        .into_iter()
        .map(|key| config["scoring"][key].as_f64().unwrap_or(f64::NAN))
        .collect::<Vec<_>>();
    if weights
        .iter()
        .any(|value| !value.is_finite() || *value < 0.0)
        || (weights.iter().sum::<f64>() - 1.0).abs() > 1e-9
    {
        bail!("scoring weights must be finite, non-negative, and sum to 1.0");
    }
    for (pointer, allowed) in [
        (
            "/check/fail_on_context_band",
            &["compact", "healthy", "warning", "critical"][..],
        ),
        (
            "/check/fail_on_slop_band",
            &["low", "moderate", "high", "critical"][..],
        ),
    ] {
        let value = config
            .pointer(pointer)
            .and_then(Value::as_str)
            .unwrap_or_default();
        if !allowed.contains(&value) {
            bail!(
                "{} has unsupported value {value:?}",
                pointer.trim_start_matches('/').replace('/', ".")
            );
        }
    }
    Ok(())
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
        let legacy = check.remove("fail_on_priority_band");
        if !check.contains_key("fail_on_slop_band") {
            if let Some(legacy) = legacy {
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
    validate_override_shape(&override_value, &default_config(), "")?;
    let mut merged = default_config();
    deep_merge(&mut merged, override_value);
    validate(&merged)?;
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

pub fn ensure_runtime_gitignore(repo_root: &Path) -> Result<bool> {
    ensure_state_dirs(repo_root)?;
    let target = slop_dir(repo_root).join(".gitignore");
    if target.exists() {
        return Ok(false);
    }
    fs::write(&target, DEFAULT_SLOP_GITIGNORE)
        .with_context(|| format!("failed to write {}", target.display()))?;
    Ok(true)
}

pub fn initialize(repo_root: &Path, force: bool) -> Result<InitResult> {
    ensure_state_dirs(repo_root)?;
    let config_target = config_path(repo_root);
    let config_status = if force || !config_target.exists() {
        fs::write(&config_target, MINIMAL_CONFIG)
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

#[cfg(test)]
mod tests {
    use std::fs;

    use serde_json::{Value, json};
    use tempfile::tempdir;

    use super::{config_path, default_config, load};

    fn load_payload(payload: Value) -> Value {
        let repository = tempdir().expect("temporary repository");
        let path = config_path(repository.path());
        fs::create_dir_all(path.parent().expect("config parent")).expect("config directory");
        fs::write(
            &path,
            serde_yaml::to_string(&payload).expect("serialize config"),
        )
        .expect("write config");
        load(repository.path()).expect("load config")
    }

    #[test]
    fn default_config_uses_the_schema_two_contract() {
        let config = default_config();

        assert_eq!(config["schema_version"], 2);
        assert_eq!(config["check"]["fail_on_slop_band"], "critical");
        assert!(config["check"].get("fail_on_priority_band").is_none());
        for section in ["tokenization", "organization", "verification"] {
            assert!(config.get(section).is_some(), "missing default {section}");
        }
    }

    #[test]
    fn schema_one_payload_defaults_and_aliases_are_normalized_to_schema_two() {
        let normalized = load_payload(json!({
            "tokenizer": {"name": "r50k_base"},
            "context_bands": {"warning_max_tokens": 9_000},
            "history": {"follow_renames": true}
        }));

        assert_eq!(normalized["schema_version"], 2);
        assert_eq!(
            normalized["tokenization"]["context_tokenizer_name"],
            "r50k_base"
        );
        assert_eq!(
            normalized["tokenization"]["context_bands"]["warning_max_tokens"],
            9_000
        );
        assert_eq!(
            normalized["tokenization"]["context_bands"]["compact_max_tokens"],
            3_072
        );
        assert_eq!(normalized["history"]["follow_renames"], true);
        assert_eq!(normalized["tokenizer"]["name"], "r50k_base");
        assert_eq!(normalized["context_bands"]["warning_max_tokens"], 9_000);
    }

    #[test]
    fn every_legacy_priority_band_maps_to_its_slop_band() {
        for (legacy, expected) in [
            ("watchlist", "low"),
            ("needs_refactor", "moderate"),
            ("should_refactor", "high"),
            ("must_refactor", "critical"),
        ] {
            let normalized = load_payload(json!({
                "schema_version": 2,
                "check": {"fail_on_priority_band": legacy}
            }));

            assert_eq!(normalized["check"]["fail_on_slop_band"], expected);
            assert!(
                normalized["check"].get("fail_on_priority_band").is_none(),
                "legacy key survived normalization for {legacy}"
            );
        }
    }

    #[test]
    fn new_slop_band_wins_and_the_legacy_key_is_always_removed() {
        let normalized = load_payload(json!({
            "schema_version": 2,
            "check": {
                "fail_on_priority_band": "must_refactor",
                "fail_on_slop_band": "moderate"
            }
        }));

        assert_eq!(normalized["check"]["fail_on_slop_band"], "moderate");
        assert!(normalized["check"].get("fail_on_priority_band").is_none());
    }

    #[test]
    fn strict_validation_rejects_unknown_keys_wrong_types_ranges_and_weights() {
        for (payload, expected) in [
            (
                json!({"schema_version": 2, "mystery": true}),
                "unknown configuration key mystery",
            ),
            (
                json!({"schema_version": 2, "history": {"churn_window_days": "many"}}),
                "wrong type",
            ),
            (
                json!({"schema_version": 2, "tokenization": {"context_bands": {"compact_max_tokens": 9000, "healthy_max_tokens": 8000}}}),
                "strictly increasing",
            ),
            (
                json!({"schema_version": 2, "organization": {"min_similarity": -1.0}}),
                "non-negative",
            ),
            (
                json!({"schema_version": 2, "scoring": {"context_weight": 0.9}}),
                "sum to 1.0",
            ),
        ] {
            let repository = tempdir().expect("temporary repository");
            let path = config_path(repository.path());
            fs::create_dir_all(path.parent().expect("config parent")).expect("config directory");
            fs::write(
                &path,
                serde_yaml::to_string(&payload).expect("serialize config"),
            )
            .expect("write config");
            let error = load(repository.path()).expect_err("invalid config must fail closed");
            assert!(error.to_string().contains(expected), "{error:#}");
        }
    }
}
