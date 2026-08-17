use std::fs;
use std::path::Path;
use std::process::Command;

use serde_yaml::Value as YamlValue;

use crate::manifest::{RELEASE_TARGETS, is_strict_semver, project_version};

pub fn validate(repo_root: &Path) -> Vec<String> {
    let mut errors = Vec::new();
    crate::workflows::validate_public_release_workflows(repo_root, &mut errors);
    validate_package_boundary(repo_root, &mut errors);
    validate_version_alignment(repo_root, &mut errors);
    validate_action_documentation(repo_root, &mut errors);
    validate_release_documentation(repo_root, &mut errors);
    validate_document_consistency(repo_root, &mut errors);
    validate_advisor_release_gate(repo_root, &mut errors);
    validate_scoop_boundary(repo_root, &mut errors);
    validate_removed_runtime_surfaces(repo_root, &mut errors);
    errors
}

fn validate_advisor_release_gate(repo_root: &Path, errors: &mut Vec<String>) {
    let relative = "benchmarks/advisor/release-gate.json";
    let gate = match fs::read(repo_root.join(relative))
        .ok()
        .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
    {
        Some(gate) => gate,
        None => {
            errors.push(format!("{relative} must contain valid JSON."));
            return;
        }
    };
    let recommendation = gate.get("recommendation").and_then(|value| value.as_str());
    let enabled = gate
        .get("public_inference_enabled")
        .and_then(|value| value.as_bool());
    if gate.get("schema_version").and_then(|value| value.as_u64()) != Some(1)
        || !matches!(recommendation, Some("ship" | "adjust" | "defer"))
        || enabled.is_none()
    {
        errors.push(format!(
            "{relative} must define schema 1, a ship/adjust/defer recommendation, and public_inference_enabled."
        ));
    }
    if enabled == Some(true) && recommendation != Some("ship") {
        errors.push(format!(
            "{relative} must fail closed: public inference requires a ship recommendation."
        ));
    }
    if gate.get("canonical_model").and_then(|value| value.as_str())
        != Some("openai/gpt-oss-safeguard-20b")
    {
        errors.push(format!(
            "{relative} must retain the benchmarked canonical Safeguard identity."
        ));
    }
    let model_size = gate
        .get("minimum_model_size_bytes")
        .and_then(|value| value.as_u64())
        .unwrap_or_default();
    let estimated_peak = gate
        .get("minimum_estimated_peak_memory_bytes")
        .and_then(|value| value.as_u64())
        .unwrap_or_default();
    let physical = gate
        .get("minimum_physical_memory_bytes")
        .and_then(|value| value.as_u64())
        .unwrap_or_default();
    let reserve = gate
        .get("minimum_available_memory_reserve_bytes")
        .and_then(|value| value.as_u64())
        .unwrap_or_default();
    let swap_growth = gate
        .get("maximum_swap_growth_bytes")
        .and_then(|value| value.as_u64())
        .unwrap_or(u64::MAX);
    let initial_swap = gate
        .get("maximum_initial_swap_used_bytes")
        .and_then(|value| value.as_u64())
        .unwrap_or(u64::MAX);
    if model_size < 13_793_441_254
        || estimated_peak < 16 * 1024 * 1024 * 1024
        || estimated_peak < model_size
        || physical < 24 * 1024 * 1024 * 1024
        || reserve < 8 * 1024 * 1024 * 1024
        || initial_swap > 256 * 1024 * 1024
        || swap_growth > 256 * 1024 * 1024
    {
        errors.push(format!(
            "{relative} weakens the checked-in model, memory, reserve, or swap safety floor."
        ));
    }
    let Some(decision_relative) = gate.get("decision_record").and_then(|value| value.as_str())
    else {
        errors.push(format!("{relative} must name its decision record."));
        return;
    };
    if decision_relative.starts_with('/') || decision_relative.contains("..") {
        errors.push(format!("{relative} decision_record must be repo-relative."));
        return;
    }
    let decision = fs::read_to_string(repo_root.join(decision_relative)).unwrap_or_default();
    let expected = format!(
        "Recommendation: **{}**",
        recommendation.unwrap_or("invalid")
    );
    if !decision.contains(&expected) {
        errors.push(format!(
            "{relative} recommendation must match {decision_relative}."
        ));
    }
    let benchmark_source = fs::read_to_string(repo_root.join("xtask/src/advisor_benchmark/run.rs"))
        .unwrap_or_default();
    if benchmark_source.contains("Command::new(\"ollama\")")
        || benchmark_source.contains("ollama_cold_model")
    {
        errors.push(
            "Advisor benchmark tooling must not start, stop, install, or otherwise manage Ollama."
                .into(),
        );
    }
    let manifest = fs::read_to_string(repo_root.join("Cargo.toml")).unwrap_or_default();
    let advisor_source =
        fs::read_to_string(repo_root.join("src/cli/advice_cmd.rs")).unwrap_or_default();
    if !advisor_features_fail_closed(&manifest)
        || !advisor_source.contains("cfg!(feature = \"advisor-inference-benchmark\")")
    {
        errors.push(
            "Public releases must keep inference behind the non-default advisor-inference-benchmark feature."
                .into(),
        );
    }
    let advisor_docs = fs::read_to_string(repo_root.join("docs/advisor.md")).unwrap_or_default();
    if recommendation == Some("defer")
        && (!advisor_docs.contains("Public inference status: **disabled**")
            || !advisor_docs.contains("provider-free context"))
    {
        errors.push(
            "docs/advisor.md must disclose the deferred gate and provider-free context path."
                .into(),
        );
    }
}

fn advisor_features_fail_closed(manifest: &str) -> bool {
    let Ok(manifest) = toml::from_str::<toml::Value>(manifest) else {
        return false;
    };
    let Some(features) = manifest.get("features").and_then(toml::Value::as_table) else {
        return false;
    };
    features
        .get("default")
        .and_then(toml::Value::as_array)
        .is_some_and(Vec::is_empty)
        && features
            .get("advisor-inference-benchmark")
            .and_then(toml::Value::as_array)
            .is_some_and(Vec::is_empty)
}

include!("distribution/document_consistency.rs");
include!("distribution/release_docs.rs");

include!("distribution/action_docs.rs");

fn validate_package_boundary(repo_root: &Path, errors: &mut Vec<String>) {
    let root_manifest = repo_root.join("Cargo.toml");
    let xtask_manifest = repo_root.join("xtask/Cargo.toml");
    let root = match fs::read_to_string(&root_manifest) {
        Ok(text) => text,
        Err(error) => {
            errors.push(format!("Unable to read Cargo.toml: {error}"));
            return;
        }
    };
    let xtask = match fs::read_to_string(&xtask_manifest) {
        Ok(text) => text,
        Err(error) => {
            errors.push(format!("Unable to read xtask/Cargo.toml: {error}"));
            return;
        }
    };
    require(&root, "members = [\".\"]", "Cargo.toml", errors);
    require(&root, "default-members = [\".\"]", "Cargo.toml", errors);
    require(&root, "exclude = [\"xtask\"]", "Cargo.toml", errors);
    require(&root, "publish = [\"crates-io\"]", "Cargo.toml", errors);
    require(&root, "build = \"build.rs\"", "Cargo.toml", errors);
    require(&root, "\"/build.rs\"", "Cargo.toml", errors);
    require(&root, "\"/src/cli/html/*.css\"", "Cargo.toml", errors);
    require(&root, "\"/src/cli/html/*.js\"", "Cargo.toml", errors);
    if root.lines().any(|line| {
        let line = line.trim();
        line.starts_with('"') && line.contains("xtask")
    }) {
        errors
            .push("The public git-slop package include list must not contain xtask paths.".into());
    }
    require(&xtask, "publish = false", "xtask/Cargo.toml", errors);
    require(&xtask, "[workspace]", "xtask/Cargo.toml", errors);
    if repo_root.join("Cargo.lock").exists()
        && fs::read_to_string(repo_root.join("Cargo.lock"))
            .is_ok_and(|lock| lock.contains("name = \"git-slop-xtask\""))
    {
        errors.push("The public Cargo.lock must not include git-slop-xtask.".into());
    }
    let xtask_lock = repo_root.join("xtask/Cargo.lock");
    if !xtask_lock.exists() {
        errors.push("xtask/Cargo.lock must be committed for the private tooling workspace.".into());
    }
}

fn validate_version_alignment(repo_root: &Path, errors: &mut Vec<String>) {
    let version = match project_version(repo_root) {
        Ok(version) => version,
        Err(error) => {
            errors.push(format!(
                "Unable to resolve Cargo.toml package version: {error}"
            ));
            return;
        }
    };
    if !is_strict_semver(&version) {
        errors.push(format!(
            "Cargo.toml package version must be strict semver, received {version}."
        ));
        return;
    }

    validate_lock_version(repo_root, &version, errors);
    if let Err(error) = crate::release::validate_release_inventory(repo_root, &version) {
        errors.push(format!("Release inventory is invalid: {error}"));
    }
    validate_action_default(repo_root, &version, errors);
    validate_installer_fallback(repo_root, &version, errors);
    validate_release_workflow_default(repo_root, &version, errors);
    for (relative, markers) in [
        (
            "README.md",
            &["coreycoto/git-slop@v", "cargo install git-slop --version "][..],
        ),
        (
            "docs/github-action.md",
            &["coreycoto/git-slop@v", "| `version` | `"][..],
        ),
        (
            "docs/install.md",
            &[
                "cargo install git-slop --version ",
                "After the bucket lists ",
            ][..],
        ),
        ("docs/archive-install.md", &["release=v"][..]),
        (
            "plugins/git-slop/skills/adopt-repo/SKILL.md",
            &["Minimal CI adoption after `", "uses: coreycoto/git-slop@v"][..],
        ),
        (
            "xtask/README.md",
            &["release-prepare --version ", "--tag v"][..],
        ),
        ("man/git-slop.1", &["\"git-slop "][..]),
    ] {
        validate_document_versions(repo_root, relative, markers, &version, errors);
    }
}

fn validate_lock_version(repo_root: &Path, version: &str, errors: &mut Vec<String>) {
    let relative = "Cargo.lock";
    let path = repo_root.join(relative);
    let text = match fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) => {
            errors.push(format!("Unable to read {relative}: {error}"));
            return;
        }
    };
    let payload = match toml::from_str::<toml::Value>(&text) {
        Ok(payload) => payload,
        Err(error) => {
            errors.push(format!("Unable to parse {relative}: {error}"));
            return;
        }
    };
    let product_versions = payload
        .get("package")
        .and_then(toml::Value::as_array)
        .into_iter()
        .flatten()
        .filter(|package| package.get("name").and_then(toml::Value::as_str) == Some("git-slop"))
        .filter_map(|package| package.get("version").and_then(toml::Value::as_str))
        .collect::<Vec<_>>();
    if product_versions != [version] {
        errors.push(format!(
            "Cargo.lock must contain exactly one git-slop package at version {version}; found {}.",
            product_versions.join(", ")
        ));
    }
}

fn validate_action_default(repo_root: &Path, version: &str, errors: &mut Vec<String>) {
    let relative = "action.yml";
    let path = repo_root.join(relative);
    let text = match fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) => {
            errors.push(format!("Unable to read {relative}: {error}"));
            return;
        }
    };
    let payload = match serde_yaml::from_str::<YamlValue>(&text) {
        Ok(payload) => payload,
        Err(error) => {
            errors.push(format!("Unable to parse {relative}: {error}"));
            return;
        }
    };
    let action_version = payload
        .get("inputs")
        .and_then(|inputs| inputs.get("version"))
        .and_then(|input| input.get("default"))
        .and_then(YamlValue::as_str);
    if action_version != Some(version) {
        errors.push(format!(
            "action.yml inputs.version.default must equal Cargo.toml version {version}."
        ));
    }
}

fn validate_installer_fallback(repo_root: &Path, version: &str, errors: &mut Vec<String>) {
    let relative = "action/install.mjs";
    let path = repo_root.join(relative);
    let text = match fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) => {
            errors.push(format!("Unable to read {relative}: {error}"));
            return;
        }
    };
    let marker = "process.env.GIT_SLOP_ACTION_VERSION || \"";
    let Some(tail) = text.split_once(marker).map(|(_, tail)| tail) else {
        errors.push(format!(
            "{relative} must define the GIT_SLOP_ACTION_VERSION fallback."
        ));
        return;
    };
    let fallback = tail.split_once('"').map(|(fallback, _)| fallback);
    if fallback != Some(version) {
        errors.push(format!(
            "{relative} release fallback must equal Cargo.toml version {version}."
        ));
    }
}

fn validate_release_workflow_default(repo_root: &Path, version: &str, errors: &mut Vec<String>) {
    let relative = ".github/workflows/release-publish.yml";
    let path = repo_root.join(relative);
    let text = match fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) => {
            errors.push(format!("Unable to read {relative}: {error}"));
            return;
        }
    };
    let payload = match serde_yaml::from_str::<YamlValue>(&text) {
        Ok(payload) => payload,
        Err(error) => {
            errors.push(format!("Unable to parse {relative}: {error}"));
            return;
        }
    };
    let release_version = payload
        .get("on")
        .and_then(|trigger| trigger.get("workflow_dispatch"))
        .and_then(|dispatch| dispatch.get("inputs"))
        .and_then(|inputs| inputs.get("version"))
        .and_then(|input| input.get("default"))
        .and_then(YamlValue::as_str);
    if release_version != Some(version) {
        errors.push(format!(
            "{relative} workflow_dispatch.inputs.version.default must equal Cargo.toml version {version}."
        ));
    }
}

fn validate_document_versions(
    repo_root: &Path,
    relative: &str,
    markers: &[&str],
    version: &str,
    errors: &mut Vec<String>,
) {
    let path = repo_root.join(relative);
    let text = match fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) => {
            errors.push(format!("Unable to read {relative}: {error}"));
            return;
        }
    };
    for marker in markers {
        let values = text
            .match_indices(marker)
            .map(|(offset, _)| {
                let tail = &text[offset + marker.len()..];
                let length = tail
                    .bytes()
                    .take_while(|byte| byte.is_ascii_digit() || *byte == b'.')
                    .count();
                &tail[..length]
            })
            .collect::<Vec<_>>();
        if values.is_empty() {
            errors.push(format!("{relative} must include version marker {marker}."));
            continue;
        }
        for found in values {
            if found != version {
                errors.push(format!(
                    "{relative} version after {marker} must equal Cargo.toml version {version}; found {found}."
                ));
            }
        }
    }
}

fn validate_scoop_boundary(repo_root: &Path, errors: &mut Vec<String>) {
    for (relative, markers) in [
        (
            "README.md",
            &[
                "scoop bucket add coreycoto https://github.com/coreycoto/scoop-bucket",
                "scoop install coreycoto/git-slop",
            ][..],
        ),
        (
            "docs/install.md",
            &[
                "https://github.com/coreycoto/scoop-bucket",
                "scoop install coreycoto/git-slop",
                "scoop update git-slop",
                "SHA256SUMS",
                "release-manifest.json",
            ][..],
        ),
        (
            "docs/release-checklist.md",
            &[
                "## Publish And Verify The External Scoop Manifest",
                "automatic trusted-main Scoop receiver",
                "git-slop-v<version>-x86_64-pc-windows-msvc.zip",
                "git-slop-v<version>-aarch64-pc-windows-msvc.zip",
                "cross-version upgrade-in-place",
                "scoop update git-slop",
                "scoop uninstall git-slop",
            ][..],
        ),
        (
            "docs/architecture.md",
            &[
                "coreycoto/scoop-bucket",
                "twelve-asset/eleven-checksum",
                "trusted-main receiver creates a manifest-only bucket pull request",
            ][..],
        ),
    ] {
        let path = repo_root.join(relative);
        let text = match fs::read_to_string(&path) {
            Ok(text) => text,
            Err(error) => {
                errors.push(format!("Unable to read {relative}: {error}"));
                continue;
            }
        };
        for marker in markers {
            require(&text, marker, relative, errors);
        }
    }

    let publish_relative = ".github/workflows/release-publish.yml";
    match fs::read_to_string(repo_root.join(publish_relative)) {
        Ok(text) => {
            for forbidden in [
                "secrets.SCOOP_BUCKET_DISPATCH_TOKEN",
                "Dispatch immutable release identity to Scoop bucket",
                "--repo coreycoto/scoop-bucket",
            ] {
                if text.contains(forbidden) {
                    errors.push(format!(
                        "{publish_relative} must remain independent of the external Scoop bucket; found privileged marker {forbidden:?}."
                    ));
                }
            }
        }
        Err(error) => errors.push(format!("Unable to read {publish_relative}: {error}")),
    }

    let relay_relative = ".github/workflows/release-published.yml";
    match fs::read_to_string(repo_root.join(relay_relative)) {
        Ok(text) => {
            for marker in [
                "Dispatch immutable release identity to Scoop bucket",
                "secrets.SCOOP_BUCKET_DISPATCH_TOKEN",
                "--repo coreycoto/scoop-bucket",
                "--field release_manifest_sha256=",
                ".immutable == true",
            ] {
                require(&text, marker, relay_relative, errors);
            }
        }
        Err(error) => errors.push(format!("Unable to read {relay_relative}: {error}")),
    }
}

fn validate_removed_runtime_surfaces(repo_root: &Path, errors: &mut Vec<String>) {
    for removed in ["pyproject.toml", "uv.lock", "src/git_slop"] {
        if repo_root.join(removed).exists() {
            errors.push(format!(
                "{removed} must be removed after the Rust xtask migration."
            ));
        }
    }
    match repository_owned_py_files(repo_root) {
        Ok(paths) => errors.extend(
            paths
                .into_iter()
                .map(|path| format!("Repository-owned .py file must be removed: {path}.")),
        ),
        Err(error) => errors.push(error),
    }
}

pub fn repository_owned_py_files(repo_root: &Path) -> Result<Vec<String>, String> {
    let output = Command::new("git")
        .args([
            "ls-files",
            "--cached",
            "--others",
            "--exclude-standard",
            "-z",
        ])
        .current_dir(repo_root)
        .output()
        .map_err(|error| format!("Unable to enumerate repository files with git: {error}"))?;
    if !output.status.success() {
        let detail = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        return Err(format!(
            "Unable to enumerate repository files with git ({}): {detail}",
            output.status
        ));
    }

    let mut paths = Vec::new();
    for raw_path in output
        .stdout
        .split(|byte| *byte == 0)
        .filter(|path| !path.is_empty())
    {
        let relative = std::str::from_utf8(raw_path).map_err(|_| {
            "Repository contains a non-UTF-8 path; the .py-file boundary cannot be verified safely."
                .to_owned()
        })?;
        let path = repo_root.join(relative);
        if fs::symlink_metadata(&path).is_ok()
            && path.extension().and_then(|extension| extension.to_str()) == Some("py")
        {
            paths.push(relative.replace('\\', "/"));
        }
    }
    paths.sort();
    paths.dedup();
    Ok(paths)
}

fn require(text: &str, expected: &str, label: &str, errors: &mut Vec<String>) {
    if !text.contains(expected) {
        errors.push(format!("{label} must include {expected}."));
    }
}

#[cfg(test)]
include!("distribution/tests.rs");
