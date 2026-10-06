include!("ci_feedback.rs");

fn validate_consumer_tool_workflows(workflows: &Path, errors: &mut Vec<String>) {
    for name in CONSUMER_TOOL_WORKFLOWS {
        let Some(text) = read(&workflows.join(name), errors) else {
            continue;
        };
        validate_recovery_concurrency(&text, name, errors);
        if name != "dependency-remediation.yml" {
            validate_prepared_package_retention(&text, name, errors);
            validate_native_plan_transport(&text, name, errors);
        }
        for required in [
            "scripts/with-gh-steward.sh --prepare",
            "scripts/with-gh-steward.sh --verify",
        ] {
            require(&text, required, name, errors);
        }
        for forbidden in [
            "AGENT_PLUGINS_READ_TOKEN",
            "AGENT_PLUGINS_GIT_TOKEN",
            "agent-plugins-private-history",
            "PEX_INTERPRETER",
            "python -m agent_plugins",
            "scripts/with-agent-plugins.sh",
            "actions/cache@",
            "RUNNER_TOOL_CACHE",
            "runner.tool_cache",
            "restore-keys:",
        ] {
            forbid(&text, forbidden, name, errors);
        }
    }

    for (name, required) in [
        (
            "dependency-remediation.yml",
            "scripts/prepare-codex-plugins.sh",
        ),
        (
            "governance-reconcile.yml",
            "scripts/prepare-codex-plugins.sh",
        ),
        ("merge-on-green.yml", "scripts/prepare-codex-plugins.sh"),
        (
            "execution_state_sync.yml",
            "\"$GH_STEWARD_BIN\" execution apply",
        ),
    ] {
        if let Some(text) = read(&workflows.join(name), errors) {
            require(&text, required, name, errors);
        }
    }
    if let Some(text) = read(&workflows.join("execution_state_sync.yml"), errors) {
        for required in [
            "\"$GH_STEWARD_BIN\" snapshot project",
            "\"$GH_STEWARD_BIN\" execution prepare",
            "runs recover",
            "runs acquire-handoff",
            "runs context-record-plan",
            "runs context-install-journal",
            "runs context-capture-journal",
            "runs finish-noop",
            "scripts/finalize-gh-steward-run.sh",
            "cancel-in-progress: false",
            "recovery_needed",
        ] {
            require(&text, required, "execution_state_sync.yml", errors);
        }
    }

    for name in PUBLIC_RELEASE_WORKFLOWS {
        if let Some(text) = read(&workflows.join(name), errors) {
            validate_no_consumer_tools(name, &text, errors);
        }
    }
}

fn validate_native_plan_transport(text: &str, name: &str, errors: &mut Vec<String>) {
    let (outer, inner) = match name {
        "execution_state_sync.yml" => ("execution-sync-prepare", "execution-sync"),
        "governance-reconcile.yml" => ("governance-prepare", "governance-apply"),
        "merge-on-green.yml" => ("merge-prepare", "merge-apply"),
        _ => return,
    };
    require(text, &format!("--outer-command {outer}"), name, errors);
    require(text, &format!("--plan-command {inner}"), name, errors);
    for required in [
        "\"$GH_STEWARD_BIN\" plan extract",
        "--input \"native-plan=",
        "--review-path",
        "--review-sha256",
    ] {
        require(text, required, name, errors);
    }
    forbid(text, "jq -S '.data'", name, errors);
}

fn validate_recovery_concurrency(text: &str, name: &str, errors: &mut Vec<String>) {
    let Ok(payload) = serde_yaml::from_str::<YamlValue>(text) else {
        errors.push(format!("{name} must contain valid workflow YAML."));
        return;
    };
    let concurrency = payload.get("concurrency");
    if concurrency
        .and_then(|value| value.get("queue"))
        .and_then(YamlValue::as_str)
        != Some("max")
        || concurrency
            .and_then(|value| value.get("cancel-in-progress"))
            .and_then(YamlValue::as_bool)
            != Some(false)
    {
        errors.push(format!(
            "{name} must use concurrency.queue: max and cancel-in-progress: false to preserve pending recovery invocations."
        ));
    }
}

fn validate_prepared_package_retention(text: &str, name: &str, errors: &mut Vec<String>) {
    let Ok(payload) = serde_yaml::from_str::<YamlValue>(text) else {
        errors.push(format!("{name} must contain valid workflow YAML."));
        return;
    };
    let Some(steps) = payload
        .get("jobs")
        .and_then(|jobs| jobs.get("apply"))
        .and_then(|job| job.get("steps"))
        .and_then(YamlValue::as_sequence)
    else {
        errors.push(format!(
            "{name} lacks native apply steps for pending package retention."
        ));
        return;
    };
    let qualifier = steps.iter().enumerate().find(|(_, step)| {
        step.get("id").and_then(YamlValue::as_str)
            == Some(if name == "execution_state_sync.yml" {
                "qualify_prepared"
            } else {
                "qualify-prepared"
            })
    });
    let upload = steps
        .iter()
        .enumerate()
        .find(|(_, step)| step.get("id").and_then(YamlValue::as_str) == Some("terminal_artifact"));
    let valid = match (qualifier, upload) {
        (Some((qualifier_index, qualifier)), Some((upload_index, upload))) => {
            let upload_if = upload.get("if").and_then(YamlValue::as_str).unwrap_or("");
            qualifier_index < upload_index
                && qualifier
                    .get("run")
                    .and_then(YamlValue::as_str)
                    .is_some_and(|run| run.contains("runs qualify-prepared"))
                && qualifier
                    .get("env")
                    .and_then(|env| env.get("GH_TOKEN"))
                    .and_then(YamlValue::as_str)
                    == Some("${{ github.token }}")
                && upload_if.contains("always()")
                && !upload_if.contains("qualify-prepared")
                && !upload_if.contains("qualify_prepared")
                && qualifier.get("if").and_then(YamlValue::as_str) == Some(upload_if)
        }
        _ => false,
    };
    if !valid {
        errors.push(format!(
            "{name} must qualify pending native work before upload and retain its package even when qualification fails."
        ));
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

struct ArtifactUploadContract {
    workflow_name: &'static str,
    job_name: &'static str,
    step_name: &'static str,
    artifact_name_fragment: &'static str,
    artifact_path_fragment: &'static str,
    retention_days: u64,
    include_hidden_files: Option<bool>,
}

fn validate_artifacts(workflows: &Path, errors: &mut Vec<String>) {
    const UPLOADS: [ArtifactUploadContract; 18] = [
        ArtifactUploadContract {
            workflow_name: "dependency-remediation.yml",
            job_name: "recover",
            step_name: "Upload immutable native recovery handoff",
            artifact_name_fragment: "-handoff-00",
            artifact_path_fragment: "${{ runner.temp }}/dependency-remediation-package",
            retention_days: 14,
            include_hidden_files: Some(true),
        },
        ArtifactUploadContract {
            workflow_name: "dependency-remediation.yml",
            job_name: "settle-noop",
            step_name: "Upload exact terminal dependency no-op artifact",
            artifact_name_fragment: "outputs.artifact_name",
            artifact_path_fragment: "${{ runner.temp }}/dependency-remediation-package",
            retention_days: 90,
            include_hidden_files: Some(true),
        },
        ArtifactUploadContract {
            workflow_name: "dependency-remediation.yml",
            job_name: "settle-noop",
            step_name: "Upload exact dependency no-op settlement checkpoint",
            artifact_name_fragment: "checkpoint_name",
            artifact_path_fragment: "checkpoint_path",
            retention_days: 90,
            include_hidden_files: None,
        },
        ArtifactUploadContract {
            workflow_name: "execution_state_sync.yml",
            job_name: "prepare",
            step_name: "Upload preview transport handoff",
            artifact_name_fragment: "-handoff-00",
            artifact_path_fragment: "${{ runner.temp }}/execution-state-package",
            retention_days: 14,
            include_hidden_files: Some(true),
        },
        ArtifactUploadContract {
            workflow_name: "execution_state_sync.yml",
            job_name: "apply",
            step_name: "Upload exact native execution recovery package",
            artifact_name_fragment: "outputs.artifact_name",
            artifact_path_fragment: "${{ runner.temp }}/execution-state-package",
            retention_days: 90,
            include_hidden_files: Some(true),
        },
        ArtifactUploadContract {
            workflow_name: "execution_state_sync.yml",
            job_name: "prepare",
            step_name: "Upload exact terminal native package",
            artifact_name_fragment: "outputs.artifact_name",
            artifact_path_fragment: "${{ runner.temp }}/execution-state-package",
            retention_days: 90,
            include_hidden_files: Some(true),
        },
        ArtifactUploadContract {
            workflow_name: "execution_state_sync.yml",
            job_name: "apply",
            step_name: "Upload the exact native settlement checkpoint",
            artifact_name_fragment: "checkpoint_name",
            artifact_path_fragment: "checkpoint_path",
            retention_days: 90,
            include_hidden_files: None,
        },
        ArtifactUploadContract {
            workflow_name: "execution_state_sync.yml",
            job_name: "prepare",
            step_name: "Upload the exact native settlement checkpoint",
            artifact_name_fragment: "checkpoint_name",
            artifact_path_fragment: "checkpoint_path",
            retention_days: 90,
            include_hidden_files: None,
        },
        ArtifactUploadContract {
            workflow_name: "governance-reconcile.yml",
            job_name: "prepare",
            step_name: "Upload exact governance handoff for apply or no-op settlement",
            artifact_name_fragment: "-handoff-00",
            artifact_path_fragment: "${{ steps.package_outputs.outputs.package }}",
            retention_days: 14,
            include_hidden_files: Some(true),
        },
        ArtifactUploadContract {
            workflow_name: "governance-reconcile.yml",
            job_name: "apply",
            step_name: "Upload exact governance recovery artifact",
            artifact_name_fragment: "outputs.artifact_name",
            artifact_path_fragment: "${{ runner.temp }}/governance-package",
            retention_days: 90,
            include_hidden_files: Some(true),
        },
        ArtifactUploadContract {
            workflow_name: "governance-reconcile.yml",
            job_name: "settle_noop",
            step_name: "Upload exact terminal governance no-op artifact",
            artifact_name_fragment: "outputs.artifact_name",
            artifact_path_fragment: "${{ runner.temp }}/governance-package",
            retention_days: 90,
            include_hidden_files: Some(true),
        },
        ArtifactUploadContract {
            workflow_name: "governance-reconcile.yml",
            job_name: "apply",
            step_name: "Upload exact governance settlement checkpoint",
            artifact_name_fragment: "checkpoint_name",
            artifact_path_fragment: "checkpoint_path",
            retention_days: 90,
            include_hidden_files: None,
        },
        ArtifactUploadContract {
            workflow_name: "governance-reconcile.yml",
            job_name: "settle_noop",
            step_name: "Upload exact governance no-op settlement checkpoint",
            artifact_name_fragment: "checkpoint_name",
            artifact_path_fragment: "checkpoint_path",
            retention_days: 90,
            include_hidden_files: None,
        },
        ArtifactUploadContract {
            workflow_name: "merge-on-green.yml",
            job_name: "prepare",
            step_name: "Upload immutable merge handoff",
            artifact_name_fragment: "-handoff-00",
            artifact_path_fragment: "${{ runner.temp }}/merge-package",
            retention_days: 14,
            include_hidden_files: Some(true),
        },
        ArtifactUploadContract {
            workflow_name: "merge-on-green.yml",
            job_name: "apply",
            step_name: "Upload exact merge recovery artifact",
            artifact_name_fragment: "outputs.artifact_name",
            artifact_path_fragment: "${{ runner.temp }}/merge-package",
            retention_days: 90,
            include_hidden_files: Some(true),
        },
        ArtifactUploadContract {
            workflow_name: "merge-on-green.yml",
            job_name: "settle_noop",
            step_name: "Upload exact terminal merge no-op artifact",
            artifact_name_fragment: "outputs.artifact_name",
            artifact_path_fragment: "${{ runner.temp }}/merge-package",
            retention_days: 90,
            include_hidden_files: Some(true),
        },
        ArtifactUploadContract {
            workflow_name: "merge-on-green.yml",
            job_name: "apply",
            step_name: "Upload exact merge settlement checkpoint",
            artifact_name_fragment: "checkpoint_name",
            artifact_path_fragment: "checkpoint_path",
            retention_days: 90,
            include_hidden_files: None,
        },
        ArtifactUploadContract {
            workflow_name: "merge-on-green.yml",
            job_name: "settle_noop",
            step_name: "Upload exact merge no-op settlement checkpoint",
            artifact_name_fragment: "checkpoint_name",
            artifact_path_fragment: "checkpoint_path",
            retention_days: 90,
            include_hidden_files: None,
        },
    ];
    for upload in UPLOADS {
        let Some(text) = read(&workflows.join(upload.workflow_name), errors) else {
            continue;
        };
        let Some(payload) = parse_ci_workflow(&text, upload.workflow_name, errors) else {
            continue;
        };
        validate_artifact_upload(&payload, &upload, errors);
    }

    if let Some(text) = read(&workflows.join("execution_state_sync.yml"), errors) {
        validate_execution_state_artifacts(&text, errors);
    }
}

fn validate_artifact_upload(
    payload: &YamlValue,
    upload: &ArtifactUploadContract,
    errors: &mut Vec<String>,
) {
    let step = payload
        .get("jobs")
        .and_then(|jobs| jobs.get(upload.job_name))
        .and_then(|job| job.get("steps"))
        .and_then(YamlValue::as_sequence)
        .and_then(|steps| {
            steps
                .iter()
                .find(|step| step.get("name").and_then(YamlValue::as_str) == Some(upload.step_name))
        });
    let valid = step.is_some_and(|step| {
        step.get("uses")
            .and_then(YamlValue::as_str)
            .is_some_and(|uses| uses.starts_with("actions/upload-artifact@"))
            && step.get("with").is_some_and(|with| {
                with.get("name")
                    .and_then(YamlValue::as_str)
                    .is_some_and(|value| value.contains(upload.artifact_name_fragment))
                    && with
                        .get("path")
                        .and_then(YamlValue::as_str)
                        .is_some_and(|value| value.contains(upload.artifact_path_fragment))
                    && with.get("retention-days").and_then(YamlValue::as_u64)
                        == Some(upload.retention_days)
                    && with.get("if-no-files-found").and_then(YamlValue::as_str) == Some("error")
                    && upload.include_hidden_files.is_none_or(|expected| {
                        with.get("include-hidden-files")
                            .and_then(YamlValue::as_bool)
                            == Some(expected)
                    })
            })
    });
    if !valid {
        errors.push(format!(
            "{} {} must retain {} with exact immutable artifact identity, path, and retention.",
            upload.workflow_name, upload.job_name, upload.step_name
        ));
    }
}

fn validate_execution_state_artifacts(text: &str, errors: &mut Vec<String>) {
    let name = "execution_state_sync.yml";
    for required in [
        "cancel-in-progress: false",
        "runs recover",
        "runs acquire-handoff",
        "runs context-record-plan",
        "runs context-install-journal",
        "runs context-capture-journal",
        "runs finish-noop",
        "scripts/finalize-gh-steward-run.sh",
        "recovery_needed",
    ] {
        require(text, required, name, errors);
    }
    forbid(text, "cancel-in-progress: true", name, errors);
    forbid(text, "scripts/recover-execution-state.sh", name, errors);
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
    validate_dogfood_analysis_clock(&text, name, errors);
    validate_dogfood_failure_evidence(&text, name, errors);

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
            "shard_dir=${manifest%.json}",
            "validate_manifest \"$shard\" \"$shard_base\"",
            "length == (unique | length)",
            "dogfood regressions exceed or drift from the reviewed acceptance ledger",
        ] {
            require(&verifier, expected, verifier_name, errors);
        }
    }
    dogfood::validate_acceptance_manifests(repo_root, errors);
}

fn validate_dogfood_analysis_clock(text: &str, name: &str, errors: &mut Vec<String>) {
    let Ok(payload) = serde_yaml::from_str::<YamlValue>(text) else {
        return;
    };
    let steps = payload
        .get("jobs")
        .and_then(|jobs| jobs.get("dogfood"))
        .and_then(|job| job.get("steps"))
        .and_then(YamlValue::as_sequence);
    let Some(steps) = steps else {
        return;
    };
    let clock = steps
        .iter()
        .position(|step| step.get("id").and_then(YamlValue::as_str) == Some("analysis-clock"));
    let valid_clock = clock.is_some_and(|index| {
        steps[index]
            .get("run")
            .and_then(YamlValue::as_str)
            .is_some_and(|run| run.contains("git show -s --format=%cI HEAD"))
    });
    let scans = steps
        .iter()
        .enumerate()
        .filter(|(_, step)| {
            step.get("run")
                .and_then(YamlValue::as_str)
                .is_some_and(|run| {
                    run.contains("target/release/git-slop find")
                        || run.contains("--repo \"$base_worktree\" find")
                })
        })
        .collect::<Vec<_>>();
    if !valid_clock
        || scans.len() != 2
        || scans.iter().any(|(index, step)| {
            clock.is_none_or(|clock| clock >= *index)
                || step
                    .get("env")
                    .and_then(|env| env.get("ANALYSIS_AS_OF"))
                    .and_then(YamlValue::as_str)
                    != Some("${{ steps.analysis-clock.outputs.as_of }}")
                || !step
                    .get("run")
                    .and_then(YamlValue::as_str)
                    .is_some_and(|run| run.contains("--as-of \"$ANALYSIS_AS_OF\""))
        })
    {
        errors.push(format!(
            "{name} must evaluate both revisions at the same exact-head analysis clock."
        ));
    }
}

fn validate_dogfood_failure_evidence(text: &str, name: &str, errors: &mut Vec<String>) {
    let enforcement = text
        .split_once("      - name: Enforce pull-request regressions")
        .and_then(|(_, tail)| {
            tail.split_once("      - name: Upload full Dogfood regression evidence on failure")
        })
        .map(|(block, _)| block);
    match enforcement {
        Some(block) => require(block, "id: regressions", name, errors),
        None => errors.push(format!(
            "{name} must identify the Dogfood regression step for failure diagnostics."
        )),
    }

    let evidence = text
        .split_once("      - name: Upload full Dogfood regression evidence on failure")
        .and_then(|(_, tail)| tail.split_once("      - name: Preview first-adoption comparison"))
        .map(|(block, _)| block);
    let Some(evidence) = evidence else {
        errors.push(format!(
            "{name} must upload complete Dogfood evidence after a regression failure."
        ));
        return;
    };
    for required in [
        "if: failure() && steps.regressions.outcome == 'failure'",
        "uses: actions/upload-artifact@",
        "git-slop-dogfood-failure-${{ github.run_id }}-${{ github.run_attempt }}",
        "${{ runner.temp }}/dogfood-comparison.json",
        ".slop/latest/report.json",
        "if-no-files-found: warn",
        "retention-days: 14",
    ] {
        require(evidence, required, name, errors);
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
    validate_consumer_tool_fixture_jobs(&texts[2].1, texts[2].0, errors);
    validate_windows_action_ci_job(&texts[1].1, texts[1].0, errors);
    validate_consumer_tool_fixtures(repo_root, errors);
}

fn validate_consumer_tool_fixture_jobs(text: &str, name: &str, errors: &mut Vec<String>) {
    const COMMAND: &str = "bash scripts/test-consumer-tooling.sh";
    let payload = match serde_yaml::from_str::<YamlValue>(text) {
        Ok(payload) => payload,
        Err(error) => {
            errors.push(format!("Unable to parse {name}: {error}"));
            return;
        }
    };
    let steps = payload
        .get("jobs")
        .and_then(|jobs| jobs.get("maintainer-contracts"))
        .and_then(|job| job.get("steps"))
        .and_then(YamlValue::as_sequence);
    let Some(steps) = steps else {
        errors.push(format!("{name} must define maintainer-contracts steps."));
        return;
    };
    if !steps.iter().any(|step| {
        step.get("run")
            .and_then(YamlValue::as_str)
            .is_some_and(|run| run.trim() == COMMAND)
    }) {
        errors.push(format!(
            "{name} maintainer-contracts job must run {COMMAND}."
        ));
    }
    let position = |command: &str| {
        steps.iter().position(|step| step.get("run").and_then(YamlValue::as_str) == Some(command))
    };
    let probe = position("scripts/with-gh-steward.sh --prepare");
    let verify = position("scripts/with-gh-steward.sh --verify");
    if position(COMMAND)
        .zip(position("cargo xtask validate"))
        .zip(probe)
        .zip(verify)
        .is_none_or(|(((fixtures, contracts), probe), verify)| fixtures >= probe || contracts >= probe || probe >= verify)
    {
        errors.push(format!("{name} must run hosted acquisition and offline receipt verification after its fixtures and validators."));
    }
    if let Some(probe) = probe {
        let env = steps[probe].get("env").and_then(YamlValue::as_mapping);
        if env.is_none_or(|env| env.len() != 1 || env.get(YamlValue::String("GH_TOKEN".into())).and_then(YamlValue::as_str) != Some("${{ github.token }}")) {
            errors.push(format!("{name} hosted acquisition must use only a step-scoped GitHub job token."));
        }
    }
    if verify.is_some_and(|i| steps[i].get("env").is_some()) {
        errors.push(format!("{name} offline receipt verification must receive no credential environment."));
    }
    let job = &payload["jobs"]["maintainer-contracts"];
    let workflow_credentials = payload.get("env").and_then(YamlValue::as_mapping).is_some_and(|env| {
        env.iter().any(|(key, value)| {
            key.as_str().is_some_and(|key| key.ends_with("_TOKEN") || key == "OPENAI_API_KEY")
                || value.as_str().is_some_and(|value| value.contains("secrets."))
        })
    });
    if workflow_credentials || job.get("env").is_some()
        || job.get("permissions").and_then(YamlValue::as_mapping).is_none_or(|permissions| {
            permissions.len() != 1 || permissions.get(YamlValue::String("contents".into())).and_then(YamlValue::as_str) != Some("read")
        })
    {
        errors.push(format!("{name} hosted acquisition probe must keep its job read-only without job-scoped credentials."));
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

fn validate_consumer_tool_fixtures(repo_root: &Path, errors: &mut Vec<String>) {
    const TESTS: [&str; 4] = [
        "with-gh-steward.test.sh",
        "prepare-codex-plugins.test.sh",
        "recover-gh-steward-run.test.sh",
        "dependency-remediation-paths.test.sh",
    ];
    for relative in TESTS {
        let path = repo_root.join("scripts").join(relative);
        let Ok(metadata) = fs::symlink_metadata(&path) else {
            errors.push(format!("scripts/{relative} must exist as a regular file."));
            continue;
        };
        if !metadata.is_file() {
            errors.push(format!("scripts/{relative} must be a regular file."));
        }
    }

    let runner_path = repo_root.join("scripts/test-consumer-tooling.sh");
    if !fs::symlink_metadata(&runner_path).is_ok_and(|metadata| metadata.is_file()) {
        errors.push("scripts/test-consumer-tooling.sh must exist as a regular file.".to_owned());
    }
    if let Some(runner) = read(&runner_path, errors) {
        validate_consumer_test_runner(&runner, errors);
    }
}

fn validate_consumer_test_runner(text: &str, errors: &mut Vec<String>) {
    let test_list = text
        .split_once("for test_script in")
        .and_then(|(_, remainder)| remainder.split_once("; do"))
        .map(|(test_list, _)| test_list)
        .unwrap_or_default();
    for test in [
        "with-gh-steward.test.sh",
        "prepare-codex-plugins.test.sh",
        "recover-gh-steward-run.test.sh",
        "dependency-remediation-paths.test.sh",
    ] {
        if !test_list
            .split_whitespace()
            .any(|entry| entry.trim_end_matches('\\') == test)
        {
            errors.push(format!("scripts/test-consumer-tooling.sh must run {test}."));
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
