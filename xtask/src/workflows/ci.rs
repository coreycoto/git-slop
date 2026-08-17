include!("ci_feedback.rs");

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

    for name in PUBLIC_RELEASE_WORKFLOWS {
        if let Some(text) = read(&workflows.join(name), errors) {
            validate_no_private_runtime(name, &text, errors);
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
        for (index, line) in text.lines().enumerate() {
            let Some((_, action)) = line.trim().split_once("uses:") else {
                continue;
            };
            let action = action.split_whitespace().next().unwrap_or_default();
            if action.starts_with("./") || action.starts_with("docker://") {
                continue;
            }
            let Some((repository, revision)) = action.rsplit_once('@') else {
                errors.push(format!(
                    "{label}:{} external Action {action} must use a full commit SHA.",
                    index + 1
                ));
                continue;
            };
            let sha_pinned = repository.contains('/')
                && revision.len() == 40
                && revision.bytes().all(|byte| byte.is_ascii_hexdigit());
            if !sha_pinned {
                errors.push(format!(
                    "{label}:{} external Action {action} must use a full commit SHA.",
                    index + 1
                ));
            }
        }
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
            "retention-days: 14",
        ] {
            require(upload, expected, name, errors);
        }
        require(
            upload,
            if name == "dependency-remediation.yml" {
                "if-no-files-found: warn"
            } else {
                "if-no-files-found: error"
            },
            name,
            errors,
        );
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
        if name == "dependency-remediation.yml" {
            for marker in [
                "Preserve Codex failure diagnostic",
                "codex_output_unavailable",
                "steps.run_codex.outcome",
            ] {
                require(&text, marker, name, errors);
            }
        }
    }

    if let Some(text) = read(&workflows.join("execution_state_sync.yml"), errors) {
        validate_execution_state_artifacts(&text, errors);
    }
}

fn validate_execution_state_artifacts(text: &str, errors: &mut Vec<String>) {
    let name = "execution_state_sync.yml";
    let artifact_root = text.find("      - name: Prepare artifact root");
    let runtime_prepare = text.find("      - name: Prepare pinned agent-plugins runtime");
    if !matches!((artifact_root, runtime_prepare), (Some(root), Some(runtime)) if root < runtime) {
        errors.push(format!(
            "{name} must create its artifact root before private runtime preparation."
        ));
    }

    let upload = text
        .split_once("      - name: Upload execution artifacts")
        .map(|(_, tail)| tail);
    let Some(upload) = upload else {
        errors.push(format!("{name} must define its artifact upload."));
        return;
    };
    for expected in [
        "if: ${{ (failure() || github.event_name == 'workflow_dispatch') && steps.artifact-root.outputs.path != '' }}",
        "path: ${{ steps.artifact-root.outputs.path }}",
        "include-hidden-files: true",
        "if-no-files-found: error",
        "retention-days: 14",
    ] {
        require(upload, expected, name, errors);
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
        "dogfood-pr-base",
        "--format json --detail full --limit 1000",
        "scripts/verify-dogfood-regressions.sh",
        "config/github/dogfood-regression-acceptances.json",
        "BASE_SHA: ${{ github.event.pull_request.base.sha }}",
        "HEAD_SHA: ${{ github.event.pull_request.head.sha }}",
        "ref: ${{ github.event_name == 'pull_request' && github.event.pull_request.head.sha || github.sha }}",
        "target/release/git-slop check --report .slop/latest/report.json --evaluate-only",
        "Scan a repository with no configuration override",
        "cat .slop/latest/health.md",
        "path: .slop/latest/health.md",
        "include-hidden-files: true",
        "retention-days: 14",
    ] {
        require(&text, expected, name, errors);
    }
    forbid(&text, "path: .slop/latest\n", name, errors);
    forbid(&text, "uv run git-slop", name, errors);
    forbid(&text, "check || true", name, errors);
    if text
        .matches("ref: ${{ github.event_name == 'pull_request' && github.event.pull_request.head.sha || github.sha }}")
        .count()
        != 2
    {
        errors.push(format!(
            "{name} must pin both Dogfood checkouts to the exact pull-request head."
        ));
    }
    let enforcement = text
        .split_once("      - name: Enforce pull-request regressions")
        .and_then(|(_, tail)| tail.split_once("      - name: Preview first-adoption comparison"))
        .map(|(block, _)| block);
    match enforcement {
        Some(block) => {
            require(
                block,
                "BASE_SHA: ${{ github.event.pull_request.base.sha }}",
                name,
                errors,
            );
            require(
                block,
                "HEAD_SHA: ${{ github.event.pull_request.head.sha }}",
                name,
                errors,
            );
        }
        None => errors.push(format!(
            "{name} must retain a bounded pull-request regression enforcement block."
        )),
    }

    let Some(repo_root) = workflows.parent().and_then(Path::parent) else {
        errors.push(format!("{name} repository root could not be resolved."));
        return;
    };
    let verifier_name = "scripts/verify-dogfood-regressions.sh";
    if let Some(verifier) = read(&repo_root.join(verifier_name), errors) {
        for expected in [
            "pagination.regressions.has_more == false",
            ".base_report.head_sha == $base",
            ".head_report.head_sha == $head",
            ".repo.head_sha == $head",
            "content_sha256",
            "maximum_slop_score",
            ".severity == \"notice\" or .severity == \"warning\"",
            "dogfood regressions exceed or drift from the reviewed acceptance ledger",
        ] {
            require(&verifier, expected, verifier_name, errors);
        }
    }

    let manifest_name = "config/github/dogfood-regression-acceptances.json";
    let Some(manifest_text) = read(&repo_root.join(manifest_name), errors) else {
        return;
    };
    let manifest: serde_json::Value = match serde_json::from_str(&manifest_text) {
        Ok(value) => value,
        Err(error) => {
            errors.push(format!("{manifest_name} is not valid JSON: {error}"));
            return;
        }
    };
    if manifest.get("schema_version") != Some(&serde_json::json!(1)) {
        errors.push(format!("{manifest_name} must use schema version 1."));
    }
    let entries = manifest
        .get("acceptances")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|acceptance| acceptance.get("entries"))
        .filter_map(serde_json::Value::as_array)
        .flatten()
        .collect::<Vec<_>>();
    if entries.is_empty() {
        errors.push(format!("{manifest_name} must contain reviewed entries."));
    }
    if entries
        .iter()
        .any(|entry| entry.get("severity") == Some(&serde_json::json!("critical")))
    {
        errors.push(format!(
            "{manifest_name} must never accept a critical regression."
        ));
    }
}

fn validate_ci(repo_root: &Path, workflows: &Path, errors: &mut Vec<String>) {
    let names = ["ci.yml", "ci-public.yml", "ci-maintainer.yml"];
    let texts = names
        .into_iter()
        .filter_map(|name| read(&workflows.join(name), errors).map(|text| (name, text)))
        .collect::<Vec<_>>();
    if texts.len() != names.len() {
        return;
    }
    let combined = texts
        .iter()
        .map(|(_, text)| text.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    for expected in [
        "concurrency:",
        "cancel-in-progress: true",
        "uses: ./.github/workflows/ci-public.yml",
        "uses: ./.github/workflows/ci-maintainer.yml",
        "public-rust:",
        "maintainer-contracts:",
        "supply-chain:",
        "action-tests:",
        "change-classification:",
        "full-validation:",
        "cargo xtask verify-changed --base \"$BASE_SHA\" --dry-run",
        "Full required validation",
        "cargo fmt -p git-slop -- --check",
        "cargo clippy -p git-slop --all-targets --all-features --locked",
        "cargo test -p git-slop --all-targets --all-features --locked",
        "cargo fmt --manifest-path xtask/Cargo.toml --all -- --check",
        "cargo clippy --manifest-path xtask/Cargo.toml --all-targets --all-features --locked",
        "cargo test --manifest-path xtask/Cargo.toml --all-targets --all-features --locked",
        "cargo package -p git-slop --locked",
        "cargo publish -p git-slop --dry-run --locked",
        "cargo xtask validate",
        "EmbarkStudios/cargo-deny-action@",
        "command: check advisories licenses sources",
        "node --test action/*.test.mjs",
        "ubuntu-24.04",
        "macos-15",
        "windows-2025",
        "windows-11-arm",
    ] {
        require(&combined, expected, "CI workflow family", errors);
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
        forbid(&combined, forbidden, "CI workflow family", errors);
    }
    validate_ci_feedback_contract(workflows, errors);
    validate_runtime_launcher_ci_job(&texts[2].1, texts[2].0, errors);
    validate_windows_action_ci_job(&texts[1].1, texts[1].0, errors);
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
    let command_is_in_maintainer_contracts = payload
        .get("jobs")
        .and_then(|jobs| jobs.get("maintainer-contracts"))
        .and_then(|job| job.get("steps"))
        .and_then(YamlValue::as_sequence)
        .is_some_and(|steps| {
            steps.iter().any(|step| {
                step.get("run")
                    .and_then(YamlValue::as_str)
                    .is_some_and(|run| run.trim() == COMMAND)
            })
        });
    if !command_is_in_maintainer_contracts {
        errors.push(format!(
            "{name} maintainer-contracts job must run {COMMAND}."
        ));
    }
}

fn validate_windows_action_ci_job(text: &str, name: &str, errors: &mut Vec<String>) {
    const PLATFORM_OSES: [&str; 4] = ["ubuntu-24.04", "macos-15", "windows-2025", "windows-11-arm"];
    const SETUP_STEP: &str = "Set up Node.js for Windows Action tests";
    const TEST_STEP: &str = "Test GitHub Action on Windows";
    const WINDOWS_CONDITION: &str = "runner.os == 'Windows'";
    const TEST_COMMAND: &str = "node --test action/install.test.mjs";

    let payload = match serde_yaml::from_str::<YamlValue>(text) {
        Ok(payload) => payload,
        Err(error) => {
            errors.push(format!("Unable to parse {name}: {error}"));
            return;
        }
    };
    let Some(jobs) = payload.get("jobs").and_then(YamlValue::as_mapping) else {
        errors.push(format!("{name} must define jobs."));
        return;
    };
    let Some(platform_smoke) = job(jobs, "platform-smoke", name, errors) else {
        return;
    };

    if platform_smoke.get("runs-on").and_then(YamlValue::as_str) != Some("${{ matrix.os }}") {
        errors.push(format!(
            "{name} platform-smoke job must use matrix.os as runs-on."
        ));
    }
    let matrix = platform_smoke
        .get("strategy")
        .and_then(|strategy| strategy.get("matrix"));
    let platform_oses = matrix
        .and_then(|matrix| matrix.get("os"))
        .and_then(YamlValue::as_sequence)
        .map(|oses| {
            oses.iter()
                .filter_map(YamlValue::as_str)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    if platform_oses.as_slice() != PLATFORM_OSES.as_slice() {
        errors.push(format!(
            "{name} platform-smoke job must define the exact supported platform matrix."
        ));
    }
    if matrix
        .and_then(|matrix| matrix.get("exclude"))
        .and_then(YamlValue::as_sequence)
        .is_some_and(|excludes| {
            excludes.iter().any(|exclude| {
                exclude
                    .get("os")
                    .and_then(YamlValue::as_str)
                    .is_some_and(|os| matches!(os, "windows-2025" | "windows-11-arm"))
            })
        })
    {
        errors.push(format!(
            "{name} platform-smoke job must not exclude either supported Windows lane."
        ));
    }

    let platform_steps = steps(platform_smoke);

    for step_name in [SETUP_STEP, TEST_STEP] {
        let count = platform_steps
            .iter()
            .filter(|step| step.get("name").and_then(YamlValue::as_str) == Some(step_name))
            .count();
        if count != 1 {
            errors.push(format!(
                "{name} platform-smoke job must define exactly one {step_name} step."
            ));
        }
    }

    let setup = named_step(platform_smoke, SETUP_STEP);
    let test = named_step(platform_smoke, TEST_STEP);

    if let Some(setup) = setup {
        if setup.get("if").and_then(YamlValue::as_str) != Some(WINDOWS_CONDITION) {
            errors.push(format!(
                "{name} {SETUP_STEP} step must use the exact Windows runner condition."
            ));
        }
        if setup.get("uses").and_then(YamlValue::as_str)
            != Some("actions/setup-node@820762786026740c76f36085b0efc47a31fe5020")
        {
            errors.push(format!(
                "{name} {SETUP_STEP} step must use the pinned actions/setup-node v7 commit."
            ));
        }
        if setup
            .get("with")
            .and_then(|with| with.get("node-version"))
            .and_then(YamlValue::as_str)
            != Some("24")
        {
            errors.push(format!("{name} {SETUP_STEP} step must install Node.js 24."));
        }
    }

    if let Some(test) = test {
        if test.get("if").and_then(YamlValue::as_str) != Some(WINDOWS_CONDITION) {
            errors.push(format!(
                "{name} {TEST_STEP} step must use the exact Windows runner condition."
            ));
        }
        if test.get("run").and_then(YamlValue::as_str).map(str::trim) != Some(TEST_COMMAND) {
            errors.push(format!(
                "{name} {TEST_STEP} step must run exactly {TEST_COMMAND}."
            ));
        }
    }

    if setup.is_some() && test.is_some() {
        let setup_position = platform_steps
            .iter()
            .position(|step| step.get("name").and_then(YamlValue::as_str) == Some(SETUP_STEP))
            .expect("named setup step must have a position");
        let test_position = platform_steps
            .iter()
            .position(|step| step.get("name").and_then(YamlValue::as_str) == Some(TEST_STEP))
            .expect("named test step must have a position");
        if setup_position >= test_position {
            errors.push(format!(
                "{name} {SETUP_STEP} step must run before {TEST_STEP}."
            ));
        }
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
