fn windows_action_ci_errors(text: &str) -> Vec<String> {
    let mut errors = Vec::new();
    validate_windows_action_ci_job(text, "ci.yml", &mut errors);
    errors
}

fn valid_windows_action_ci() -> &'static str {
    r#"jobs:
  platform-smoke:
    strategy:
      matrix:
        os:
          - ubuntu-24.04
          - macos-15
          - windows-2025
          - windows-11-arm
    runs-on: ${{ matrix.os }}
    steps:
      - name: Set up Node.js for Windows Action tests
        if: runner.os == 'Windows'
        uses: actions/setup-node@820762786026740c76f36085b0efc47a31fe5020
        with:
          node-version: "24"
      - name: Test GitHub Action on Windows
        if: runner.os == 'Windows'
        run: node --test action/install.test.mjs
"#
}

#[test]
fn windows_action_ci_contract_accepts_node_24_test() {
    assert_eq!(
        windows_action_ci_errors(valid_windows_action_ci()),
        Vec::<String>::new()
    );
}

#[test]
fn windows_action_ci_contract_rejects_node_version_drift() {
    let drifted = valid_windows_action_ci().replace("node-version: \"24\"", "node-version: \"22\"");
    let errors = windows_action_ci_errors(&drifted).join("\n");
    assert!(errors.contains("must install Node.js 24"), "{errors}");
}

#[test]
fn windows_action_ci_contract_rejects_condition_drift() {
    let drifted = valid_windows_action_ci().replacen(
        "if: runner.os == 'Windows'",
        "if: runner.os != 'Windows'",
        1,
    );
    let errors = windows_action_ci_errors(&drifted).join("\n");
    assert!(
        errors.contains("must use the exact Windows runner condition"),
        "{errors}"
    );
}

#[test]
fn windows_action_ci_contract_rejects_command_drift() {
    let drifted = valid_windows_action_ci().replace(
        "node --test action/install.test.mjs",
        "node --test action/*.test.mjs",
    );
    let errors = windows_action_ci_errors(&drifted).join("\n");
    assert!(errors.contains("must run exactly"), "{errors}");
}

#[test]
fn windows_action_ci_contract_rejects_missing_windows_x64_lane() {
    let drifted = valid_windows_action_ci().replace("          - windows-2025\n", "");
    let errors = windows_action_ci_errors(&drifted).join("\n");
    assert!(
        errors.contains("exact supported platform matrix"),
        "{errors}"
    );
}

#[test]
fn windows_action_ci_contract_rejects_missing_windows_arm64_lane() {
    let drifted = valid_windows_action_ci().replace("          - windows-11-arm\n", "");
    let errors = windows_action_ci_errors(&drifted).join("\n");
    assert!(
        errors.contains("exact supported platform matrix"),
        "{errors}"
    );
}

#[test]
fn windows_action_ci_contract_rejects_wrong_runs_on() {
    let drifted =
        valid_windows_action_ci().replace("runs-on: ${{ matrix.os }}", "runs-on: windows-2025");
    let errors = windows_action_ci_errors(&drifted).join("\n");
    assert!(errors.contains("must use matrix.os as runs-on"), "{errors}");
}

#[test]
fn windows_action_ci_contract_rejects_excluded_windows_lane() {
    let drifted = valid_windows_action_ci().replace(
            "          - windows-11-arm\n    runs-on:",
            "          - windows-11-arm\n        exclude:\n          - os: windows-11-arm\n    runs-on:",
        );
    let errors = windows_action_ci_errors(&drifted).join("\n");
    assert!(
        errors.contains("must not exclude either supported Windows lane"),
        "{errors}"
    );
}

#[test]
fn windows_action_ci_contract_rejects_missing_setup() {
    let missing_setup = r#"jobs:
  platform-smoke:
    strategy:
      matrix:
        os:
          - ubuntu-24.04
          - macos-15
          - windows-2025
          - windows-11-arm
    runs-on: ${{ matrix.os }}
    steps:
      - name: Test GitHub Action on Windows
        if: runner.os == 'Windows'
        run: node --test action/install.test.mjs
"#;
    let errors = windows_action_ci_errors(missing_setup).join("\n");
    assert!(
        errors.contains("must define exactly one Set up Node.js for Windows Action tests"),
        "{errors}"
    );
}

#[test]
fn windows_action_ci_contract_rejects_reordered_setup() {
    let reordered = r#"jobs:
  platform-smoke:
    strategy:
      matrix:
        os:
          - ubuntu-24.04
          - macos-15
          - windows-2025
          - windows-11-arm
    runs-on: ${{ matrix.os }}
    steps:
      - name: Test GitHub Action on Windows
        if: runner.os == 'Windows'
        run: node --test action/install.test.mjs
      - name: Set up Node.js for Windows Action tests
        if: runner.os == 'Windows'
        uses: actions/setup-node@v7
        with:
          node-version: "24"
"#;
    let errors = windows_action_ci_errors(reordered).join("\n");
    assert!(
        errors.contains("must run before Test GitHub Action on Windows"),
        "{errors}"
    );
}

#[test]
fn execution_state_artifacts_require_durable_recovery_and_target_identity() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let valid =
        fs::read_to_string(root.join(".github/workflows/execution_state_sync.yml")).unwrap();
    let mut errors = Vec::new();
    validate_execution_state_artifacts(&valid, &mut errors);
    assert_eq!(errors, Vec::<String>::new());

    for (drifted, expected) in [
        (
            valid.replace("cancel-in-progress: false", "cancel-in-progress: true"),
            "cancel-in-progress: false",
        ),
        (
            valid.replace("runs context-capture-journal", "runs capture-journal"),
            "runs context-capture-journal",
        ),
        (
            format!("{valid}\nscripts/recover-execution-state.sh --recover\n"),
            "must not include scripts/recover-execution-state.sh",
        ),
    ] {
        let mut errors = Vec::new();
        validate_execution_state_artifacts(&drifted, &mut errors);
        assert!(
            errors.iter().any(|error| error.contains(expected)),
            "missing {expected}: {errors:?}"
        );
    }
}

#[test]
fn recovery_workflows_preserve_pending_invocations() {
    for name in CONSUMER_TOOL_WORKFLOWS {
        let valid = workflow_text(name);
        let mut errors = Vec::new();
        validate_recovery_concurrency(&valid, name, &mut errors);
        assert!(errors.is_empty(), "{name}: {errors:?}");
        for drifted in [
            valid.replace("queue: max", "queue: single"),
            valid.replace("  queue: max\n", ""),
            valid.replace("queue: max", "queue: {max: 100}"),
            valid.replace("cancel-in-progress: false", "cancel-in-progress: true"),
        ] {
            let mut errors = Vec::new();
            validate_recovery_concurrency(&drifted, name, &mut errors);
            assert!(
                errors.iter().any(|error| error.contains("preserve pending recovery invocations")),
                "{name} accepted discarded or invalid queued runs: {errors:?}"
            );
        }
    }
}

#[test]
fn consumer_tooling_runner_executes_native_adapters_and_path_fixtures() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let runner = fs::read_to_string(root.join("scripts/test-consumer-tooling.sh")).unwrap();
    let mut errors = Vec::new();
    validate_consumer_test_runner(&runner, &mut errors);
    assert_eq!(errors, Vec::<String>::new());

    let drifted = runner.replace("recover-gh-steward-run.test.sh \\\n", "");
    let mut errors = Vec::new();
    validate_consumer_test_runner(&drifted, &mut errors);
    assert!(
        errors
            .iter()
            .any(|error| error.contains("must run recover-gh-steward-run.test.sh")),
        "{errors:?}"
    );
}

#[test]
fn native_workflows_retain_pending_packages_after_qualification_failure() {
    for name in ["execution_state_sync.yml", "governance-reconcile.yml", "merge-on-green.yml"] {
        let valid = workflow_text(name);
        let mut errors = Vec::new();
        validate_prepared_package_retention(&valid, name, &mut errors);
        assert!(errors.is_empty(), "{name}: {errors:?}");
        for drifted in [
            valid.replace("id: qualify-prepared", "id: obsolete-qualifier"),
            valid.replace("runs qualify-prepared", "runs finish-noop"),
            valid.replace("GH_TOKEN: ${{ github.token }}", "GH_TOKEN: unrelated"),
            valid.replace("always() && needs.prepare.outputs.artifact_name", "steps.qualify-prepared.outcome == 'success' && needs.prepare.outputs.artifact_name"),
        ] {
            let mut errors = Vec::new();
            validate_prepared_package_retention(&drifted, name, &mut errors);
            assert!(
                errors.iter().any(|error| error.contains("retain its package even when qualification fails")),
                "{name} accepted lost pending-work evidence: {errors:?}"
            );
        }
    }
}

#[test]
fn native_prepared_policy_binds_each_plan_to_its_own_mutator() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let policy: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(root.join(".agents/gh-steward-recovery-policy.json")).unwrap(),
    ).unwrap();
    for (workflow, plan, job, step) in [
        ("execution_state_sync.yml", "execution", "Apply only the immutable reviewed execution plan", "Apply or recover exact reviewed execution plan"),
        ("governance-reconcile.yml", "label-palette", "Apply only the exact reviewed governance plan", "Apply exact reviewed label palette plan"),
        ("merge-on-green.yml", "merge", "Apply only the exact reviewed merge plan", "Apply reviewed exact-head merge plan"),
    ] {
        let plans = policy["workflows"][workflow]["plans"].as_object().unwrap();
        assert_eq!(plans.values().filter(|value| value.get("prepared_recovery").is_some()).count(), 1);
        assert_eq!(
            plans[plan]["prepared_recovery"]["mutators"],
            serde_json::json!([{"job": job, "steps": [step]}]),
            "{workflow}/{plan} must observe its own mutation-capable step",
        );
    }
}

#[test]
fn release_publish_workflow_is_exactly_generated_from_stage_fragments() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let rendered = render_release_workflow(root).expect("render release workflow");
    assert_eq!(rendered, workflow_text("release-publish.yml"));
    generate_release_workflow(root, true).expect("generated workflow is current");
}
