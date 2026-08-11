use std::fs;
use std::path::Path;
use std::process::Command;

use serde_yaml::Value as YamlValue;

use crate::manifest::{is_strict_semver, project_version};

pub fn validate(repo_root: &Path) -> Vec<String> {
    let mut errors = Vec::new();
    crate::workflows::validate_public_release_workflows(repo_root, &mut errors);
    validate_package_boundary(repo_root, &mut errors);
    validate_version_alignment(repo_root, &mut errors);
    validate_action_documentation(repo_root, &mut errors);
    validate_scoop_boundary(repo_root, &mut errors);
    validate_removed_runtime_surfaces(repo_root, &mut errors);
    errors
}

fn markdown_table_names(document: &str, heading: &str) -> std::collections::BTreeSet<String> {
    let Some(section) = document.split(heading).nth(1) else {
        return std::collections::BTreeSet::new();
    };
    section
        .split("\n## ")
        .next()
        .unwrap_or(section)
        .lines()
        .filter_map(|line| line.strip_prefix("| `"))
        .filter_map(|line| line.split('`').next())
        .map(str::to_string)
        .collect()
}

fn validate_action_documentation(repo_root: &Path, errors: &mut Vec<String>) {
    let action = match fs::read_to_string(repo_root.join("action.yml"))
        .ok()
        .and_then(|text| serde_yaml::from_str::<YamlValue>(&text).ok())
    {
        Some(action) => action,
        None => {
            errors.push("action.yml must be readable YAML for documentation validation.".into());
            return;
        }
    };
    let docs = match fs::read_to_string(repo_root.join("docs/github-action.md")) {
        Ok(docs) => docs,
        Err(error) => {
            errors.push(format!("Unable to read docs/github-action.md: {error}"));
            return;
        }
    };
    for (field, heading) in [("inputs", "## Inputs"), ("outputs", "## Outputs")] {
        let expected = action[field]
            .as_mapping()
            .into_iter()
            .flat_map(|mapping| mapping.keys())
            .filter_map(YamlValue::as_str)
            .map(str::to_string)
            .collect::<std::collections::BTreeSet<_>>();
        let documented = markdown_table_names(&docs, heading);
        if expected != documented {
            errors.push(format!(
                "docs/github-action.md {field} table must exactly match action.yml: expected={expected:?}, documented={documented:?}"
            ));
        }
    }
}

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
                "release=v",
                "cargo install git-slop --version ",
                "After the bucket lists ",
            ][..],
        ),
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
                "ten-asset/nine-checksum",
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
        Ok(text) if text.to_ascii_lowercase().contains("scoop") => errors.push(format!(
            "{publish_relative} must remain independent of the external Scoop bucket."
        )),
        Ok(_) => {}
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
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn version_fixture() -> TempDir {
        let temp = TempDir::new().unwrap();
        let root = temp.path();
        fs::create_dir_all(root.join("action")).unwrap();
        fs::create_dir_all(root.join("docs")).unwrap();
        fs::create_dir_all(root.join("man")).unwrap();
        fs::create_dir_all(root.join(".github/workflows")).unwrap();
        fs::create_dir_all(root.join("plugins/git-slop/skills/adopt-repo")).unwrap();
        fs::create_dir_all(root.join("xtask")).unwrap();
        fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"git-slop\"\nversion = \"0.9.0\"\n",
        )
        .unwrap();
        fs::write(
            root.join("Cargo.lock"),
            "version = 4\n\n[[package]]\nname = \"git-slop\"\nversion = \"0.9.0\"\n",
        )
        .unwrap();
        fs::write(
            root.join("action.yml"),
            "inputs:\n  version:\n    default: \"0.9.0\"\n",
        )
        .unwrap();
        fs::write(
            root.join("action/install.mjs"),
            "const version = process.env.GIT_SLOP_ACTION_VERSION || \"0.9.0\";\n",
        )
        .unwrap();
        fs::write(
            root.join(".github/workflows/release-publish.yml"),
            "on:\n  workflow_dispatch:\n    inputs:\n      version:\n        default: \"0.9.0\"\n",
        )
        .unwrap();
        fs::write(
            root.join("README.md"),
            "uses: coreycoto/git-slop@v0.9.0\n\
             cargo install git-slop --version 0.9.0\n",
        )
        .unwrap();
        fs::write(
            root.join("docs/github-action.md"),
            "uses: coreycoto/git-slop@v0.9.0\n\n## Inputs\n\n| Input | Default | Purpose |\n| --- | --- | --- |\n| `version` | `0.9.0` | Version |\n\n## Outputs\n\n| Output | Purpose |\n| --- | --- |\n",
        )
        .unwrap();
        fs::write(
            root.join("docs/install.md"),
            "release=v0.9.0\n\
             cargo install git-slop --version 0.9.0\n\
             After the bucket lists 0.9.0\n",
        )
        .unwrap();
        fs::write(
            root.join("plugins/git-slop/skills/adopt-repo/SKILL.md"),
            "Minimal CI adoption after `0.9.0` is published:\nuses: coreycoto/git-slop@v0.9.0\n",
        )
        .unwrap();
        fs::write(
            root.join("xtask/README.md"),
            "cargo xtask release-prepare --version 0.9.0\ncargo xtask release-manifest --tag v0.9.0\n",
        )
        .unwrap();
        fs::write(
            root.join("man/git-slop.1"),
            ".TH GIT-SLOP 1 \"today\" \"git-slop 0.9.0\"\n",
        )
        .unwrap();
        temp
    }

    #[test]
    fn repository_distribution_contract_passes() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
        assert_eq!(validate(root), Vec::<String>::new());
    }

    #[test]
    fn version_alignment_covers_structured_and_documented_surfaces() {
        let temp = version_fixture();
        let mut errors = Vec::new();
        validate_version_alignment(temp.path(), &mut errors);
        assert_eq!(errors, Vec::<String>::new());

        fs::write(
            temp.path().join("Cargo.lock"),
            "version = 4\n\n[[package]]\nname = \"git-slop\"\nversion = \"0.9.1\"\n",
        )
        .unwrap();
        fs::write(
            temp.path().join("action.yml"),
            "inputs:\n  version:\n    default: \"0.9.1\"\n",
        )
        .unwrap();
        fs::write(
            temp.path().join("action/install.mjs"),
            "const version = process.env.GIT_SLOP_ACTION_VERSION || \"0.9.1\";\n",
        )
        .unwrap();
        fs::write(
            temp.path().join(".github/workflows/release-publish.yml"),
            "on:\n  workflow_dispatch:\n    inputs:\n      version:\n        default: \"0.9.1\"\n",
        )
        .unwrap();
        fs::write(
            temp.path().join("README.md"),
            "uses: coreycoto/git-slop@v0.9.1\n\
             cargo install git-slop --version 0.9.1\n",
        )
        .unwrap();
        fs::write(
            temp.path().join("docs/install.md"),
            "release=v0.9.1\n\
             cargo install git-slop --version 0.9.1\n\
             After the bucket lists 0.9.1\n",
        )
        .unwrap();
        fs::write(
            temp.path()
                .join("plugins/git-slop/skills/adopt-repo/SKILL.md"),
            "Minimal CI adoption after `0.9.1` is published:\nuses: coreycoto/git-slop@v0.9.1\n",
        )
        .unwrap();
        fs::write(
            temp.path().join("xtask/README.md"),
            "cargo xtask release-prepare --version 0.9.1\ncargo xtask release-manifest --tag v0.9.1\n",
        )
        .unwrap();

        let mut errors = Vec::new();
        validate_version_alignment(temp.path(), &mut errors);
        let rendered = errors.join("\n");
        for expected in [
            "Cargo.lock",
            "action.yml",
            "action/install.mjs",
            ".github/workflows/release-publish.yml",
            "README.md",
            "docs/install.md",
            "plugins/git-slop/skills/adopt-repo/SKILL.md",
            "xtask/README.md",
        ] {
            assert!(
                rendered.contains(expected),
                "missing {expected}: {rendered}"
            );
        }
    }

    #[test]
    fn removed_runtime_check_covers_the_entire_owned_repository() {
        let temp = TempDir::new().unwrap();
        let root = temp.path();
        assert!(
            Command::new("git")
                .args(["init", "--quiet"])
                .current_dir(root)
                .status()
                .unwrap()
                .success()
        );
        fs::create_dir_all(root.join(".github")).unwrap();
        fs::create_dir_all(root.join("ignored")).unwrap();
        fs::write(root.join(".gitignore"), "/ignored/\n").unwrap();
        fs::write(root.join("root_helper.py"), "pass\n").unwrap();
        fs::write(root.join(".github/contract.py"), "pass\n").unwrap();
        fs::write(root.join("ignored/external.py"), "pass\n").unwrap();

        assert_eq!(
            repository_owned_py_files(root).unwrap(),
            [".github/contract.py", "root_helper.py"]
        );
    }

    #[test]
    fn scoop_contract_stays_documented_and_external() {
        let temp = TempDir::new().unwrap();
        let root = temp.path();
        fs::create_dir_all(root.join("docs")).unwrap();
        fs::create_dir_all(root.join(".github/workflows")).unwrap();
        fs::write(
            root.join("README.md"),
            "scoop bucket add coreycoto https://github.com/coreycoto/scoop-bucket\n\
             scoop install coreycoto/git-slop\n",
        )
        .unwrap();
        fs::write(
            root.join("docs/install.md"),
            "https://github.com/coreycoto/scoop-bucket\n\
             scoop install coreycoto/git-slop\n\
             scoop update git-slop\n\
             SHA256SUMS\n\
             release-manifest.json\n",
        )
        .unwrap();
        fs::write(
            root.join("docs/release-checklist.md"),
            "## Publish And Verify The External Scoop Manifest\n\
             automatic trusted-main Scoop receiver\n\
             git-slop-v<version>-x86_64-pc-windows-msvc.zip\n\
             git-slop-v<version>-aarch64-pc-windows-msvc.zip\n\
             cross-version upgrade-in-place\n\
             scoop update git-slop\n\
             scoop uninstall git-slop\n",
        )
        .unwrap();
        fs::write(
            root.join("docs/architecture.md"),
            "coreycoto/scoop-bucket\n\
             ten-asset/nine-checksum\n\
             trusted-main receiver creates a manifest-only bucket pull request\n",
        )
        .unwrap();
        fs::write(
            root.join(".github/workflows/release-publish.yml"),
            "name: Release\n",
        )
        .unwrap();
        fs::write(
            root.join(".github/workflows/release-published.yml"),
            "name: Verify release\n\
             Dispatch immutable release identity to Scoop bucket\n\
             secrets.SCOOP_BUCKET_DISPATCH_TOKEN\n\
             --repo coreycoto/scoop-bucket\n\
             --field release_manifest_sha256=\n\
             .immutable == true\n",
        )
        .unwrap();

        let mut errors = Vec::new();
        validate_scoop_boundary(root, &mut errors);
        assert_eq!(errors, Vec::<String>::new());

        fs::write(
            root.join(".github/workflows/release-publish.yml"),
            "name: Dispatch Scoop update\n",
        )
        .unwrap();
        fs::write(
            root.join("docs/install.md"),
            "https://github.com/coreycoto/scoop-bucket\n\
             scoop install coreycoto/git-slop\n\
             SHA256SUMS\n\
             release-manifest.json\n",
        )
        .unwrap();

        let mut errors = Vec::new();
        validate_scoop_boundary(root, &mut errors);
        let rendered = errors.join("\n");
        assert!(rendered.contains("scoop update git-slop"), "{rendered}");
        assert!(
            rendered.contains("must remain independent of the external Scoop bucket"),
            "{rendered}"
        );
    }
}
