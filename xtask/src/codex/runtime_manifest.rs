use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use serde_json::Value as JsonValue;

use super::{EXPECTED_PLUGIN_URL, json_string, load_json, read_text};

pub(super) const INSTALLED_PLUGIN_NAME: &str = "project-management-workflows";
pub(super) const EXPECTED_MARKETPLACE_NAME: &str = "agent-plugins-marketplace";
pub(super) const EXPECTED_PLUGIN_SHA: &str = "e42f887045a2460a5d33b41bedb0565e5fa0d75d";
pub(super) const MARKETPLACE_SOURCE_MANIFEST: &str = ".agents/plugins/marketplace-source.json";
pub(super) const EXPECTED_RUNTIME_REPOSITORY: &str = "coreycoto/agent-plugins";
pub(super) const EXPECTED_RUNTIME_TAG: &str = "v0.1.0";
pub(super) const EXPECTED_RUNTIME_VERSION: &str = "0.1.0";
pub(super) const EXPECTED_RUNTIME_TARGET: &str = "x86_64-unknown-linux-gnu";
pub(super) const EXPECTED_RUNTIME_ARCHIVE: &str =
    "agent-plugins-v0.1.0-x86_64-unknown-linux-gnu.tar.gz";
pub(super) const EXPECTED_RUNTIME_MEMBER: &str =
    "agent-plugins-v0.1.0-x86_64-unknown-linux-gnu/agent-plugins";
pub(super) const EXPECTED_RUNTIME_SHA256: &str =
    "40a2d2a1ad21f262b27832c3eb2ad046360c8295e5bb7f6efaba1e3ea933a6f0";
pub(super) const EXPECTED_RUNTIME_SIZE: u64 = 42_846_550;
pub(super) const EXPECTED_RELEASE_MANIFEST: &str = "release-manifest.json";
pub(super) const EXPECTED_CHECKSUMS: &str = "SHA256SUMS";
pub(super) const AGENT_PLUGIN_WRAPPER: &str = "scripts/with-agent-plugins.sh";

pub(super) fn validate_marketplace_source(repo_root: &Path, errors: &mut Vec<String>) {
    if let Some(manifest) = load_json(repo_root, MARKETPLACE_SOURCE_MANIFEST, errors) {
        validate_marketplace_source_manifest(&manifest, errors);
    }
}

pub(super) fn validate_marketplace_source_manifest(manifest: &JsonValue, errors: &mut Vec<String>) {
    if json_string(manifest, "marketplace_name") != Some(EXPECTED_MARKETPLACE_NAME) {
        errors.push(
            "Consumer bootstrap manifest must use the agent-plugins marketplace name.".into(),
        );
    }
    if json_string(manifest, "source_url") != Some(EXPECTED_PLUGIN_URL) {
        errors
            .push("Consumer bootstrap manifest must point at coreycoto/agent-plugins.git.".into());
    }
    match manifest.get("ref") {
        Some(JsonValue::String(revision)) if is_lower_hex(revision, 40) => {
            if revision != EXPECTED_PLUGIN_SHA {
                errors.push(
                    "Consumer bootstrap manifest must pin the expected agent-plugins commit."
                        .into(),
                );
            }
        }
        _ => errors.push(
            "Consumer bootstrap manifest must pin an immutable lowercase 40-character source \
             revision."
                .into(),
        ),
    }
    if json_string(manifest, "required_plugin") != Some(INSTALLED_PLUGIN_NAME) {
        errors.push(
            "Consumer bootstrap manifest must require the project-management-workflows plugin."
                .into(),
        );
    }

    let Some(runtime) = manifest
        .get("runtime_release")
        .and_then(JsonValue::as_object)
    else {
        errors.push("Consumer bootstrap manifest must define a runtime_release object.".into());
        return;
    };

    let expected_keys = [
        "archive",
        "checksums",
        "member",
        "release_manifest",
        "repository",
        "sha256",
        "size",
        "tag",
        "target",
        "version",
    ]
    .into_iter()
    .collect::<BTreeSet<_>>();
    let actual_keys = runtime.keys().map(String::as_str).collect::<BTreeSet<_>>();
    if actual_keys != expected_keys {
        errors.push(format!(
            "Consumer runtime_release must define exactly the canonical fields; found \
             {actual_keys:?}."
        ));
    }

    for (key, expected) in [
        ("repository", EXPECTED_RUNTIME_REPOSITORY),
        ("tag", EXPECTED_RUNTIME_TAG),
        ("version", EXPECTED_RUNTIME_VERSION),
        ("target", EXPECTED_RUNTIME_TARGET),
        ("archive", EXPECTED_RUNTIME_ARCHIVE),
        ("member", EXPECTED_RUNTIME_MEMBER),
        ("release_manifest", EXPECTED_RELEASE_MANIFEST),
        ("checksums", EXPECTED_CHECKSUMS),
    ] {
        if runtime.get(key).and_then(JsonValue::as_str) != Some(expected) {
            errors.push(format!(
                "Consumer runtime_release.{key} must equal {expected}."
            ));
        }
    }

    match runtime.get("sha256") {
        Some(JsonValue::String(digest)) if digest == EXPECTED_RUNTIME_SHA256 => {}
        Some(JsonValue::String(digest)) if is_lower_hex(digest, 64) => errors.push(
            "Consumer runtime_release.sha256 must pin the expected v0.1.0 archive SHA-256.".into(),
        ),
        _ => errors.push(
            "Consumer runtime_release.sha256 must pin an exact lowercase 64-character SHA-256."
                .into(),
        ),
    }
    match runtime.get("size").and_then(JsonValue::as_u64) {
        Some(size) if size == EXPECTED_RUNTIME_SIZE => {}
        Some(size) if size > 0 => errors.push(
            "Consumer runtime_release.size must pin the expected v0.1.0 archive byte count.".into(),
        ),
        _ => errors.push(
            "Consumer runtime_release.size must pin the positive archive byte count as a number."
                .into(),
        ),
    }
}

pub(super) fn validate_agent_plugin_wrapper(repo_root: &Path, errors: &mut Vec<String>) {
    let relative = AGENT_PLUGIN_WRAPPER;
    let Some(text) = read_text(repo_root, relative, errors) else {
        return;
    };
    validate_agent_plugin_wrapper_text(relative, &text, errors);

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        if let Ok(metadata) = fs::metadata(repo_root.join(relative))
            && metadata.permissions().mode() & 0o111 == 0
        {
            errors.push(format!(
                "{relative} must be executable because workflows invoke it directly."
            ));
        }
    }
}

pub(super) fn validate_agent_plugin_wrapper_text(
    relative: &str,
    text: &str,
    errors: &mut Vec<String>,
) {
    for (required, description) in [
        (
            MARKETPLACE_SOURCE_MANIFEST,
            "read the consumer-owned release manifest",
        ),
        (
            "AGENT_PLUGINS_RUNTIME_ROOT",
            "support the isolated runtime-root override",
        ),
        ("RUNNER_TEMP", "default runtime installation to RUNNER_TEMP"),
        (
            "agent-plugins-runtime",
            "use the canonical ephemeral runtime root",
        ),
        (
            "must not use RUNNER_TOOL_CACHE or an Actions cache",
            "reject persistent Actions cache roots",
        ),
        ("--prepare", "provide explicit release acquisition"),
        ("--verify", "provide offline runtime verification"),
        (
            "AGENT_PLUGINS_READ_TOKEN",
            "use the dedicated read-only acquisition credential",
        ),
        (
            "gh release download",
            "download exact private release assets",
        ),
        (
            EXPECTED_RELEASE_MANIFEST,
            "verify the publisher release manifest",
        ),
        (EXPECTED_CHECKSUMS, "verify the publisher checksum file"),
        ("sha256sum", "compute archive and installed-runtime digests"),
        (
            ".source_revision == $revision",
            "cross-check the publisher source revision with the consumer pin",
        ),
        (
            ".sha256 == $sha256",
            "cross-check the standalone artifact digest with the consumer pin",
        ),
        (
            ".size == $size",
            "cross-check the standalone artifact size with the consumer pin",
        ),
        (
            "pex-lock-from-hashed-requirements",
            "verify the publisher runtime dependency lock format",
        ),
        (
            "runtime archive SHA-256 mismatch",
            "fail closed when downloaded archive bytes differ from the consumer digest",
        ),
        (
            "runtime archive size mismatch",
            "fail closed when downloaded archive size differs from the consumer pin",
        ),
        ("--source-revision", "verify embedded source provenance"),
        (
            "installed runtime source revision mismatch",
            "fail closed when embedded source provenance differs",
        ),
        ("--version", "verify the embedded runtime version"),
        (
            "PEX_INTERPRETER=1",
            "map the compatibility python command to PEX interpreter mode",
        ),
        (
            "unset AGENT_PLUGINS_READ_TOKEN",
            "remove acquisition credentials before publisher code executes",
        ),
        (
            "isolated interpreter import and provenance smoke",
            "smoke compatibility imports in an isolated PEX interpreter",
        ),
    ] {
        if !text.contains(required) {
            errors.push(format!("{relative} must {description}."));
        }
    }

    if !shell_function_body(text, "exec_runtime").is_some_and(|body| {
        body.contains("exec env")
            && body.contains("\"$runtime_executable\" \"$@\"")
            && !body.contains("PEX_INTERPRETER=1")
            && !body.contains("GH_TOKEN")
            && !body.contains("GITHUB_TOKEN")
    }) {
        errors.push(format!(
            "{relative} must pass normal marketplace and github commands directly to the \
             verified runtime."
        ));
    }
    if !shell_function_body(text, "exec_python_compatibility").is_some_and(|body| {
        let explicitly_unsets_secrets = body.contains("AGENT_PLUGINS_READ_TOKEN")
            && body.contains("GH_TOKEN")
            && body.contains("GITHUB_TOKEN");
        let starts_from_empty_environment = body.contains("env -i")
            && body.contains("exec \"${clean_environment[@]}\"")
            && !body.contains("AGENT_PLUGINS_READ_TOKEN")
            && !body.contains("GH_TOKEN")
            && !body.contains("GITHUB_TOKEN");
        body.contains("PEX_INTERPRETER=1")
            && body.contains("\"$runtime_executable\" \"$@\"")
            && (explicitly_unsets_secrets || starts_from_empty_environment)
    }) {
        errors.push(format!(
            "{relative} must pass python compatibility arguments to the verified PEX interpreter."
        ));
    }

    if text.matches("gh release download").count() != 1 {
        errors.push(format!(
            "{relative} must contain exactly one private release download command."
        ));
    }
    match shell_function_body(text, "download_release_assets") {
        Some(body) if body.contains("gh release download") => {}
        _ => errors.push(format!(
            "{relative} must isolate the release download in download_release_assets()."
        )),
    }
    match shell_function_body(text, "prepare_runtime") {
        Some(body) if body.contains("download_release_assets") => {}
        _ => errors.push(format!(
            "{relative} must make prepare_runtime() the sole download caller."
        )),
    }
    if text.matches("download_release_assets").count() != 2 {
        errors.push(format!(
            "{relative} must reference download_release_assets only in its definition and \
             prepare_runtime()."
        ));
    }
    if text.matches("prepare_runtime").count() != 2 {
        errors.push(format!(
            "{relative} must reference prepare_runtime only in its definition and --prepare arm."
        ));
    }
    let prepare_arm_is_explicit = text
        .split_once("--prepare)")
        .and_then(|(_, tail)| tail.split_once(";;"))
        .is_some_and(|(arm, _)| arm.contains("prepare_runtime"));
    if !prepare_arm_is_explicit {
        errors.push(format!(
            "{relative} must call prepare_runtime only from the explicit --prepare dispatch arm."
        ));
    }

    for forbidden in [
        "agent-plugins @ git+",
        "git ",
        "insteadOf",
        "uv ",
        "pip ",
        "python -m ",
        "python3 ",
        "actions/setup-python",
        "curl ",
        "wget ",
    ] {
        if text.contains(forbidden) {
            errors.push(format!(
                "{relative} must not use legacy or persistent acquisition path {forbidden}."
            ));
        }
    }
}

fn shell_function_body<'a>(text: &'a str, name: &str) -> Option<&'a str> {
    let marker = format!("{name}() {{");
    let (_, tail) = text.split_once(&marker)?;
    let (body, _) = tail.split_once("\n}")?;
    Some(body)
}

fn is_lower_hex(value: &str, expected_len: usize) -> bool {
    value.len() == expected_len
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}
