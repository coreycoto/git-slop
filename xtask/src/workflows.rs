use std::fs;
use std::path::Path;

const CODEX_WORKFLOWS: [&str; 4] = [
    "dependency-remediation.yml",
    "docs-taxonomy.yml",
    "governance-reconcile.yml",
    "merge-on-green.yml",
];

pub fn validate(repo_root: &Path) -> Vec<String> {
    let mut errors = Vec::new();
    let workflows = repo_root.join(".github/workflows");

    for name in CODEX_WORKFLOWS {
        let Some(text) = read(&workflows.join(name), &mut errors) else {
            continue;
        };
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
        require(
            &text,
            "scripts/with-agent-plugins.sh python -m agent_plugins.marketplace.bootstrap install",
            name,
            &mut errors,
        );
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

    for name in ["docs-taxonomy.yml", "merge-on-green.yml"] {
        if let Some(text) = read(&workflows.join(name), &mut errors) {
            forbid(&text, "gpt-5.4-nano", name, &mut errors);
            require(&text, r#""--model","gpt-5.6-luna""#, name, &mut errors);
        }
    }

    validate_action_versions(repo_root, &workflows, &mut errors);
    validate_artifacts(&workflows, &mut errors);
    validate_dogfood(&workflows, &mut errors);
    validate_ci(&workflows, &mut errors);

    errors
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

fn validate_ci(workflows: &Path, errors: &mut Vec<String>) {
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
}
