    #[test]
    fn runtime_launcher_test_must_run_in_maintainer_contracts_job() {
        let valid = r#"jobs:
  maintainer-contracts:
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
  maintainer-contracts:
    steps:
      - run: cargo test
"#;
        let mut errors = Vec::new();
        validate_runtime_launcher_ci_job(wrong_job, "ci.yml", &mut errors);
        assert!(
            errors
                .iter()
                .any(|error| error.contains("maintainer-contracts job must run"))
        );

        let expanded_command = r#"jobs:
  maintainer-contracts:
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
                .any(|error| error.contains("maintainer-contracts job must run"))
        );
    }

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
        let drifted =
            valid_windows_action_ci().replace("node-version: \"24\"", "node-version: \"22\"");
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
    fn execution_state_artifacts_require_early_root_and_guarded_upload() {
        let valid = r#"jobs:
  sync:
    steps:
      - name: Prepare artifact root
      - name: Prepare pinned agent-plugins runtime
      - name: Upload execution artifacts
        if: ${{ (failure() || github.event_name == 'workflow_dispatch') && steps.artifact-root.outputs.path != '' }}
        with:
          path: ${{ steps.artifact-root.outputs.path }}
          include-hidden-files: true
          if-no-files-found: error
          retention-days: 14
"#;
        let mut errors = Vec::new();
        validate_execution_state_artifacts(valid, &mut errors);
        assert_eq!(errors, Vec::<String>::new());

        let late_and_unguarded = valid
            .replace(
                "      - name: Prepare artifact root\n      - name: Prepare pinned agent-plugins runtime",
                "      - name: Prepare pinned agent-plugins runtime\n      - name: Prepare artifact root",
            )
            .replace(
                "if: ${{ (failure() || github.event_name == 'workflow_dispatch') && steps.artifact-root.outputs.path != '' }}",
                "if: ${{ failure() }}",
            );
        let mut errors = Vec::new();
        validate_execution_state_artifacts(&late_and_unguarded, &mut errors);
        assert!(
            errors
                .iter()
                .any(|error| error.contains("before private runtime preparation"))
        );
        assert!(
            errors
                .iter()
                .any(|error| error.contains("artifact-root.outputs.path != ''"))
        );
    }

    #[test]
    fn release_publish_workflow_is_exactly_generated_from_stage_fragments() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
        let rendered = render_release_workflow(root).expect("render release workflow");
        assert_eq!(rendered, workflow_text("release-publish.yml"));
        generate_release_workflow(root, true).expect("generated workflow is current");
    }
