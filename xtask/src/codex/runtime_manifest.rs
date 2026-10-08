use std::fs;
use std::path::Path;

use serde_json::Value as JsonValue;

use super::{json_string, load_json, read_text};

pub(super) const PUBLIC_PLUGIN_SOURCE: &str = "coreycoto/agent-plugins";
pub(super) const PUBLIC_PLUGIN_SOURCE_SHA: &str = "df46d47e1fb0ae29424a93943be2ab55be24400a";
pub(super) const PUBLIC_PLUGIN_MARKETPLACE: &str = "agent-plugins";
pub(super) const PUBLIC_PLUGIN_NAMES: [&str; 2] = ["project-management", "product-development"];
pub(super) const CODEX_CLI_VERSION: &str = "0.160.0";
pub(super) const MARKETPLACE_SOURCE_MANIFEST: &str = ".agents/plugins/marketplace-source.json";
pub(super) const GH_STEWARD_LOCK_MANIFEST: &str = ".agents/gh-steward.lock.json";
pub(super) const GH_STEWARD_REPOSITORY: &str = "coreycoto/gh-steward";
pub(super) const GH_STEWARD_VERSION: &str = "0.6.2";
pub(super) const GH_STEWARD_WRAPPER: &str = "scripts/with-gh-steward.sh";
pub(super) const CODEX_PLUGIN_SETUP: &str = "scripts/prepare-codex-plugins.sh";

pub(super) fn validate_marketplace_source(repo_root: &Path, errors: &mut Vec<String>) {
    if let Some(manifest) = load_json(repo_root, MARKETPLACE_SOURCE_MANIFEST, errors) {
        validate_marketplace_source_manifest(&manifest, errors);
    }
}

pub(super) fn validate_marketplace_source_manifest(manifest: &JsonValue, errors: &mut Vec<String>) {
    let expected_keys = [
        "codex_cli_version",
        "marketplace_name",
        "plugins",
        "ref",
        "repository",
        "schema_version",
    ];
    if !has_exact_keys(manifest, &expected_keys) {
        errors.push(format!(
            "{MARKETPLACE_SOURCE_MANIFEST} must contain exactly the public Agent Plugins source fields."
        ));
    }
    if manifest.get("schema_version").and_then(JsonValue::as_u64) != Some(1) {
        errors.push(format!(
            "{MARKETPLACE_SOURCE_MANIFEST} must use schema_version 1."
        ));
    }
    for (key, expected) in [
        ("repository", PUBLIC_PLUGIN_SOURCE),
        ("marketplace_name", PUBLIC_PLUGIN_MARKETPLACE),
        ("ref", PUBLIC_PLUGIN_SOURCE_SHA),
        ("codex_cli_version", CODEX_CLI_VERSION),
    ] {
        if json_string(manifest, key) != Some(expected) {
            errors.push(format!(
                "{MARKETPLACE_SOURCE_MANIFEST} must pin {key} to {expected}."
            ));
        }
    }
    if manifest
        .get("plugins")
        .and_then(JsonValue::as_array)
        .is_none_or(|plugins| {
            plugins.len() != PUBLIC_PLUGIN_NAMES.len()
                || plugins
                    .iter()
                    .zip(PUBLIC_PLUGIN_NAMES)
                    .any(|(plugin, expected)| plugin.as_str() != Some(expected))
        })
    {
        errors.push(format!(
            "{MARKETPLACE_SOURCE_MANIFEST} must select project-management and product-development in order."
        ));
    }
}

pub(super) fn validate_gh_steward_lock(repo_root: &Path, errors: &mut Vec<String>) {
    let Some(lock) = load_json(repo_root, GH_STEWARD_LOCK_MANIFEST, errors) else {
        return;
    };
    validate_gh_steward_lock_manifest(&lock, errors);
}

pub(super) fn validate_gh_steward_lock_manifest(lock: &JsonValue, errors: &mut Vec<String>) {
    let expected_keys = [
        "asset_sha256",
        "repository",
        "schema_version",
        "source_revision",
        "version",
    ];
    if !has_exact_keys(lock, &expected_keys) {
        errors.push(format!(
            "{GH_STEWARD_LOCK_MANIFEST} must contain exactly the versioned source and asset pins."
        ));
    }
    if lock.get("schema_version").and_then(JsonValue::as_u64) != Some(1) {
        errors.push(format!(
            "{GH_STEWARD_LOCK_MANIFEST} must use schema_version 1."
        ));
    }
    for (key, expected) in [
        ("repository", GH_STEWARD_REPOSITORY),
        ("version", GH_STEWARD_VERSION),
    ] {
        if json_string(lock, key) != Some(expected) {
            errors.push(format!(
                "{GH_STEWARD_LOCK_MANIFEST} must pin {key} to {expected}."
            ));
        }
    }
    if lock
        .get("source_revision")
        .and_then(JsonValue::as_str)
        .is_none_or(|revision| !is_lower_hex(revision, 40))
    {
        errors.push(format!(
            "{GH_STEWARD_LOCK_MANIFEST} must contain a qualified lowercase source commit."
        ));
    }
    let expected_targets = ["darwin/amd64", "darwin/arm64", "linux/amd64", "linux/arm64"];
    let Some(assets) = lock.get("asset_sha256").and_then(JsonValue::as_object) else {
        errors.push(format!(
            "{GH_STEWARD_LOCK_MANIFEST} must contain all four supported release asset digests."
        ));
        return;
    };
    if assets.len() != expected_targets.len()
        || expected_targets
            .iter()
            .any(|target| !assets.contains_key(*target))
    {
        errors.push(format!(
            "{GH_STEWARD_LOCK_MANIFEST} must contain all four supported release asset digests."
        ));
    }
    if expected_targets.iter().any(|target| {
        assets
            .get(*target)
            .and_then(JsonValue::as_str)
            .is_none_or(|digest| !is_lower_hex(digest, 64))
    }) {
        errors.push(format!(
            "{GH_STEWARD_LOCK_MANIFEST} must pin valid SHA-256 digests for Darwin and Linux amd64 and arm64."
        ));
    }
}

pub(super) fn validate_release_acquisition_wrappers(repo_root: &Path, errors: &mut Vec<String>) {
    let Some(tool_wrapper) = read_text(repo_root, GH_STEWARD_WRAPPER, errors) else {
        return;
    };
    for (required, description) in [
        (
            GH_STEWARD_LOCK_MANIFEST,
            "read the consumer-owned exact source lock",
        ),
        (
            "coreycoto/gh-steward",
            "pin the expected public tool repository",
        ),
        (
            "scripts/release/acquire-gh-steward.sh",
            "use the source-owned attested acquisition helper",
        ),
        (
            "source_revision",
            "bind acquisition to the exact source commit",
        ),
        (
            "asset_sha256",
            "bind acquisition to the platform asset digest",
        ),
        (
            "--verify",
            "provide an offline staged-binary verification mode",
        ),
        ("source_dirty == false", "reject a dirty release binary"),
        (
            "env -u GH_TOKEN -u GITHUB_TOKEN",
            "keep repository credentials out of public source acquisition",
        ),
    ] {
        if !tool_wrapper.contains(required) {
            errors.push(format!("{GH_STEWARD_WRAPPER} must {description}."));
        }
    }
    for forbidden in [
        "AGENT_PLUGINS_READ_TOKEN",
        "PEX_INTERPRETER",
        "python -m agent_plugins",
        "gh extension install",
        "RUNNER_TOOL_CACHE",
    ] {
        if tool_wrapper.contains(forbidden) {
            errors.push(format!(
                "{GH_STEWARD_WRAPPER} must not contain {forbidden}."
            ));
        }
    }

    let Some(plugin_setup) = read_text(repo_root, CODEX_PLUGIN_SETUP, errors) else {
        return;
    };
    for (required, description) in [
        (
            MARKETPLACE_SOURCE_MANIFEST,
            "validate the public plugin source lock",
        ),
        (
            PUBLIC_PLUGIN_SOURCE_SHA,
            "use the reviewed immutable public source revision",
        ),
        (
            "readonly expected_version=\"0.160.0\"",
            "install the exact qualified Codex CLI version",
        ),
        (
            "--sparse .agents/plugins",
            "load the public first-party marketplace",
        ),
        (
            "project-management",
            "install the project-management plugin",
        ),
        (
            "product-development",
            "install the product-development plugin",
        ),
        ("plugins/gh-steward", "load the selected gh-steward plugin"),
        (
            "RUNNER_TEMP",
            "confine Codex installation to ephemeral runner storage",
        ),
    ] {
        if !plugin_setup.contains(required) {
            errors.push(format!("{CODEX_PLUGIN_SETUP} must {description}."));
        }
    }
    for forbidden in [
        "agent-plugins-private-history",
        "AGENT_PLUGINS_READ_TOKEN",
        "AGENT_PLUGINS_GIT_TOKEN",
        "gh extension install",
    ] {
        if plugin_setup.contains(forbidden) {
            errors.push(format!(
                "{CODEX_PLUGIN_SETUP} must not contain {forbidden}."
            ));
        }
    }

    validate_executable(repo_root, GH_STEWARD_WRAPPER, errors);
    validate_executable(repo_root, CODEX_PLUGIN_SETUP, errors);
}

fn validate_executable(repo_root: &Path, relative: &str, errors: &mut Vec<String>) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if fs::metadata(repo_root.join(relative))
            .is_ok_and(|metadata| metadata.permissions().mode() & 0o111 == 0)
        {
            errors.push(format!("{relative} must be executable."));
        }
    }
}

fn has_exact_keys(value: &JsonValue, expected: &[&str]) -> bool {
    value.as_object().is_some_and(|object| {
        object.len() == expected.len() && expected.iter().all(|key| object.contains_key(*key))
    })
}

fn is_lower_hex(value: &str, length: usize) -> bool {
    value.len() == length
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}
