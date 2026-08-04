use std::fs;
use std::path::Path;

use serde_yaml::Value as YamlValue;

const CODEX_WORKFLOWS: [&str; 4] = [
    "dependency-remediation.yml",
    "docs-taxonomy.yml",
    "governance-reconcile.yml",
    "merge-on-green.yml",
];

const AGENT_PLUGIN_WORKFLOWS: [&str; 5] = [
    "dependency-remediation.yml",
    "docs-taxonomy.yml",
    "governance-reconcile.yml",
    "merge-on-green.yml",
    "execution_state_sync.yml",
];

const AGENT_PLUGIN_WRAPPER: &str = "scripts/with-agent-plugins.sh";
const PREPARE_COMMAND: &str = "scripts/with-agent-plugins.sh --prepare";
const VERIFY_COMMAND: &str = "scripts/with-agent-plugins.sh --verify";
const MARKETPLACE_COMMAND: &str = "scripts/with-agent-plugins.sh marketplace install";

pub fn validate(repo_root: &Path) -> Vec<String> {
    let mut errors = Vec::new();
    let workflows = repo_root.join(".github/workflows");

    for name in CODEX_WORKFLOWS {
        let Some(text) = read(&workflows.join(name), &mut errors) else {
            continue;
        };
        if name == "dependency-remediation.yml" {
            for trusted_snapshot in [
                r#"codex_home="$RUNNER_TEMP/codex-runtime/.codex""#,
                r#"cp .codex/config.toml "$codex_home/config.toml""#,
                r#"cp .codex/*.config.toml "$codex_home/""#,
                r#"cp -R .codex/agents/. "$codex_home/agents/""#,
                "cp .github/codex/prompts/dependency-remediation.md",
                "cp .github/codex/schemas/dependency-remediation.json",
                "prompt-file: ${{ runner.temp }}/dependency-remediation-trusted/dependency-remediation.md",
                "output-schema-file: ${{ runner.temp }}/dependency-remediation-trusted/dependency-remediation.json",
            ] {
                require(&text, trusted_snapshot, name, &mut errors);
            }
        } else {
            require(
                &text,
                r#"cp .codex/config.toml "$RUNNER_TEMP/codex-runtime/.codex/config.toml""#,
                name,
                &mut errors,
            );
            require(
                &text,
                r#"cp .codex/*.config.toml "$RUNNER_TEMP/codex-runtime/.codex/""#,
                name,
                &mut errors,
            );
        }
        require(&text, MARKETPLACE_COMMAND, name, &mut errors);
        require(
            &text,
            "codex-home: ${{ runner.temp }}/codex-runtime/.codex",
            name,
            &mut errors,
        );
        require(&text, r#""--profile","ci_mutation""#, name, &mut errors);
        require(&text, "cargo xtask validate-codex", name, &mut errors);
        forbid(&text, "uv sync", name, &mut errors);
        forbid(
            &text,
            "scripts/validate_codex_surface.py",
            name,
            &mut errors,
        );
    }

    validate_agent_plugin_runtime(&workflows, &mut errors);

    for name in ["docs-taxonomy.yml", "merge-on-green.yml"] {
        if let Some(text) = read(&workflows.join(name), &mut errors) {
            forbid(&text, "gpt-5.4-nano", name, &mut errors);
            require(&text, r#""--model","gpt-5.6-luna""#, name, &mut errors);
        }
    }

    validate_action_versions(repo_root, &workflows, &mut errors);
    validate_artifacts(&workflows, &mut errors);
    validate_dogfood(&workflows, &mut errors);
    validate_ci(repo_root, &workflows, &mut errors);

    errors
}

fn validate_agent_plugin_runtime(workflows: &Path, errors: &mut Vec<String>) {
    for name in AGENT_PLUGIN_WORKFLOWS {
        let Some(text) = read(&workflows.join(name), errors) else {
            continue;
        };
        for required in [
            PREPARE_COMMAND,
            VERIFY_COMMAND,
            "AGENT_PLUGINS_READ_TOKEN: ${{ secrets.AGENT_PLUGINS_READ_TOKEN }}",
        ] {
            require(&text, required, name, errors);
        }
        for forbidden in [
            "actions/setup-python",
            "python-version:",
            "python -m pip",
            "pip install",
            "Install uv",
            "uv run",
            "uv sync",
            "AGENT_PLUGINS_GIT_TOKEN",
            "python -m agent_plugins",
            "python -c \"from agent_plugins",
            "actions/cache@",
            "RUNNER_TOOL_CACHE",
            "runner.tool_cache",
            "restore-keys:",
        ] {
            forbid(&text, forbidden, name, errors);
        }
    }

    if let Some(text) = read(&workflows.join("execution_state_sync.yml"), errors) {
        require(
            &text,
            "scripts/with-agent-plugins.sh github project-snapshot",
            "execution_state_sync.yml",
            errors,
        );
        require(
            &text,
            "scripts/with-agent-plugins.sh github execution-state",
            "execution_state_sync.yml",
            errors,
        );
    }

    if let Some(text) = read(&workflows.join("release-publish.yml"), errors) {
        for forbidden in [
            "AGENT_PLUGINS_READ_TOKEN",
            "AGENT_PLUGINS_GIT_TOKEN",
            AGENT_PLUGIN_WRAPPER,
            ".agents/plugins/marketplace-source.json",
            "coreycoto/agent-plugins",
            "agent-plugins-marketplace",
            "agent-plugins-runtime",
        ] {
            forbid(&text, forbidden, "release-publish.yml", errors);
        }
    }
}

fn validate_action_versions(repo_root: &Path, workflows: &Path, errors: &mut Vec<String>) {
    let mut surfaces = vec![repo_root.join("action.yml")];
    if let Ok(entries) = fs::read_dir(workflows) {
        surfaces.extend(entries.flatten().map(|entry| entry.path()).filter(|path| {
            path.extension().and_then(|extension| extension.to_str()) == Some("yml")
        }));
    }
    for path in surfaces {
        let Some(text) = read(&path, errors) else {
            continue;
        };
        let label = relative(repo_root, &path);
        forbid(&text, "actions/upload-artifact@v4", &label, errors);
        forbid(&text, "actions/upload-artifact@v5", &label, errors);
    }
}

fn validate_artifacts(workflows: &Path, errors: &mut Vec<String>) {
    let contracts: [(&str, &[&str]); 4] = [
        (
            "dependency-remediation.yml",
            &[
                ".artifacts/codex/dependency-remediation.json",
                ".artifacts/dependency-remediation/",
            ],
        ),
        (
            "docs-taxonomy.yml",
            &[
                ".artifacts/codex/docs-taxonomy.json",
                ".artifacts/docs-taxonomy/",
            ],
        ),
        (
            "governance-reconcile.yml",
            &[
                ".artifacts/codex/governance-reconcile.json",
                ".artifacts/github-governance/",
            ],
        ),
        (
            "merge-on-green.yml",
            &[".artifacts/codex/merge-on-green.json"],
        ),
    ];

    for (name, expected_paths) in contracts {
        let Some(text) = read(&workflows.join(name), errors) else {
            continue;
        };
        let upload = text
            .split_once("      - name: Upload ")
            .map(|(_, tail)| tail);
        let Some(upload) = upload else {
            errors.push(format!("{name} must define an upload step."));
            continue;
        };
        for expected in [
            "steps.codex_preflight.outputs.enabled == 'true'",
            "always()",
            "include-hidden-files: true",
            "if-no-files-found: error",
            "retention-days: 14",
        ] {
            require(upload, expected, name, errors);
        }
        forbid(upload, "          path: .artifacts\n", name, errors);
        for path in expected_paths {
            require(upload, path, name, errors);
        }
        if name == "merge-on-green.yml" {
            require(
                upload,
                "steps.merge_preflight.outputs.eligible == 'true'",
                name,
                errors,
            );
        }
    }

    if let Some(text) = read(&workflows.join("execution_state_sync.yml"), errors) {
        let upload = text
            .split_once("      - name: Upload execution artifacts")
            .map(|(_, tail)| tail);
        let Some(upload) = upload else {
            errors.push("execution_state_sync.yml must define its artifact upload.".into());
            return;
        };
        for expected in [
            "path: ${{ steps.artifact-root.outputs.path }}",
            "include-hidden-files: true",
            "if-no-files-found: error",
            "retention-days: 14",
        ] {
            require(upload, expected, "execution_state_sync.yml", errors);
        }
    }
}

fn validate_dogfood(workflows: &Path, errors: &mut Vec<String>) {
    let name = "dogfood.yml";
    let Some(text) = read(&workflows.join(name), errors) else {
        return;
    };
    for expected in [
        "cargo build -p git-slop --release --locked",
        "target/release/git-slop find",
        "cat .slop/latest/health.md",
        "path: .slop/latest/health.md",
        "include-hidden-files: true",
        "retention-days: 14",
    ] {
        require(&text, expected, name, errors);
    }
    forbid(&text, "path: .slop/latest\n", name, errors);
    forbid(&text, "uv run git-slop", name, errors);
}

fn validate_ci(repo_root: &Path, workflows: &Path, errors: &mut Vec<String>) {
    let name = "ci.yml";
    let Some(text) = read(&workflows.join(name), errors) else {
        return;
    };
    for expected in [
        "cargo fmt -p git-slop -- --check",
        "cargo clippy -p git-slop --all-targets --all-features --locked",
        "cargo test -p git-slop --all-targets --all-features --locked",
        "cargo fmt --manifest-path xtask/Cargo.toml --all -- --check",
        "cargo clippy --manifest-path xtask/Cargo.toml --all-targets --all-features --locked",
        "cargo test --manifest-path xtask/Cargo.toml --all-targets --all-features --locked",
        "cargo package -p git-slop --locked",
        "cargo publish -p git-slop --dry-run --locked",
        "cargo xtask validate",
        "node --test action/*.test.mjs",
        "ubuntu-24.04",
        "macos-15",
        "windows-2025",
        "windows-11-arm",
    ] {
        require(&text, expected, name, errors);
    }
    for forbidden in [
        "maintainer-tooling:",
        "Python maintainer tooling",
        "uv sync",
        "uv run pytest",
        "scripts/smoke_plugin_consumer.py",
        "tests/unit/agent_tools",
        "python -m git_slop",
        "macos-15-intel",
        "uv build",
    ] {
        forbid(&text, forbidden, name, errors);
    }
    validate_runtime_launcher_ci_job(&text, name, errors);
    validate_runtime_launcher_fixture(repo_root, errors);
}

fn validate_runtime_launcher_ci_job(text: &str, name: &str, errors: &mut Vec<String>) {
    const COMMAND: &str = "bash scripts/with-agent-plugins.test.sh";
    let payload = match serde_yaml::from_str::<YamlValue>(text) {
        Ok(payload) => payload,
        Err(error) => {
            errors.push(format!("Unable to parse {name}: {error}"));
            return;
        }
    };
    let command_is_in_rust_quality = payload
        .get("jobs")
        .and_then(|jobs| jobs.get("rust-quality"))
        .and_then(|job| job.get("steps"))
        .and_then(YamlValue::as_sequence)
        .is_some_and(|steps| {
            steps.iter().any(|step| {
                step.get("run")
                    .and_then(YamlValue::as_str)
                    .is_some_and(|run| run.trim() == COMMAND)
            })
        });
    if !command_is_in_rust_quality {
        errors.push(format!("{name} rust-quality job must run {COMMAND}."));
    }
}

fn validate_runtime_launcher_fixture(repo_root: &Path, errors: &mut Vec<String>) {
    let relative = "scripts/with-agent-plugins.test.sh";
    let path = repo_root.join(relative);
    let Ok(metadata) = fs::symlink_metadata(&path) else {
        errors.push(format!(
            "{relative} must exist as a regular executable file."
        ));
        return;
    };
    if !metadata.is_file() {
        errors.push(format!("{relative} must be a regular file."));
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        if metadata.permissions().mode() & 0o111 == 0 {
            errors.push(format!(
                "{relative} must be executable as part of the runtime-launcher test contract."
            ));
        }
    }
}

fn read(path: &Path, errors: &mut Vec<String>) -> Option<String> {
    match fs::read_to_string(path) {
        Ok(text) => Some(text),
        Err(error) => {
            errors.push(format!("Unable to read {}: {error}", path.display()));
            None
        }
    }
}

fn require(text: &str, expected: &str, label: &str, errors: &mut Vec<String>) {
    if !text.contains(expected) {
        errors.push(format!("{label} must include {expected}."));
    }
}

fn forbid(text: &str, forbidden: &str, label: &str, errors: &mut Vec<String>) {
    if text.contains(forbidden) {
        errors.push(format!("{label} must not include {forbidden}."));
    }
}

fn relative(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repository_workflows_pass() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
        assert_eq!(validate(root), Vec::<String>::new());
    }

    #[test]
    fn runtime_launcher_test_must_run_in_rust_quality_job() {
        let valid = r#"jobs:
  rust-quality:
    steps:
      - run: bash scripts/with-agent-plugins.test.sh
"#;
        let mut errors = Vec::new();
        validate_runtime_launcher_ci_job(valid, "ci.yml", &mut errors);
        assert_eq!(errors, Vec::<String>::new());

        let wrong_job = r#"jobs:
  workflow-lint:
    steps:
      - run: bash scripts/with-agent-plugins.test.sh
  rust-quality:
    steps:
      - run: cargo test
"#;
        let mut errors = Vec::new();
        validate_runtime_launcher_ci_job(wrong_job, "ci.yml", &mut errors);
        assert!(
            errors
                .iter()
                .any(|error| error.contains("rust-quality job must run"))
        );

        let expanded_command = r#"jobs:
  rust-quality:
    steps:
      - run: |
          echo preparing
          bash scripts/with-agent-plugins.test.sh
"#;
        let mut errors = Vec::new();
        validate_runtime_launcher_ci_job(expanded_command, "ci.yml", &mut errors);
        assert!(
            errors
                .iter()
                .any(|error| error.contains("rust-quality job must run"))
        );
    }
}
