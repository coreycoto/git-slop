#[test]
fn runtime_workflows_reject_unsafe_pull_request_checkout_ordering() {
    let dependency = safe_dependency_workflow();
    let mut errors = Vec::new();
    validate_agent_plugin_workflow_text(
        "dependency-remediation.yml",
        dependency,
        AgentPluginWorkflowKind::Marketplace,
        &mut errors,
    );
    assert_eq!(errors, Vec::<String>::new());

    let unsafe_dependency = dependency
        .replace(
            "      - name: Checkout requested head\n        uses: actions/checkout@v6\n        with:\n          persist-credentials: false\n          ref: ${{ github.event.pull_request.head.sha }}\n",
            "",
        )
        .replace(
            "      - name: Validate trusted Codex surface",
            "      - name: Checkout requested head\n        uses: actions/checkout@v6\n        with:\n          persist-credentials: false\n          ref: ${{ github.event.pull_request.head.sha }}\n      - name: Validate trusted Codex surface",
        );
    let mut errors = Vec::new();
    validate_agent_plugin_workflow_text(
        "dependency-remediation.yml",
        &unsafe_dependency,
        AgentPluginWorkflowKind::Marketplace,
        &mut errors,
    );
    assert!(
        errors
            .iter()
            .any(|error| error.contains("trusted base content"))
    );

    let persisted_credentials = dependency.replacen(
        "          persist-credentials: false\n",
        "          persist-credentials: true\n",
        1,
    );
    let mut errors = Vec::new();
    validate_agent_plugin_workflow_text(
        "dependency-remediation.yml",
        &persisted_credentials,
        AgentPluginWorkflowKind::Marketplace,
        &mut errors,
    );
    assert!(
        errors
            .iter()
            .any(|error| error.contains("trusted-base checkout must disable")),
        "{errors:?}"
    );

    let missing_rust_setup = dependency.replace(
        "      - name: Set up Rust\n        uses: dtolnay/rust-toolchain@trusted\n",
        "",
    );
    let mut errors = Vec::new();
    validate_agent_plugin_workflow_text(
        "dependency-remediation.yml",
        &missing_rust_setup,
        AgentPluginWorkflowKind::Marketplace,
        &mut errors,
    );
    assert!(
        errors
            .iter()
            .any(|error| error.contains("one trusted Rust setup")),
        "{errors:?}"
    );

    let extra_checkout = dependency.replace(
        "      - name: Validate trusted Codex surface",
        "      - name: Unexpected checkout\n        uses: actions/checkout@v6\n        with:\n          persist-credentials: false\n          ref: refs/heads/other\n      - name: Validate trusted Codex surface",
    );
    let mut errors = Vec::new();
    validate_agent_plugin_workflow_text(
        "dependency-remediation.yml",
        &extra_checkout,
        AgentPluginWorkflowKind::Marketplace,
        &mut errors,
    );
    assert!(
        errors.iter().any(|error| {
            error.contains("exactly the trusted-base and requested-head checkouts")
        }),
        "{errors:?}"
    );

    let head_owned_config = dependency.replace(
        "      - name: Run Codex remediation",
        "      - name: Replace trusted config\n        run: cp .codex/config.toml /tmp/config.toml\n      - name: Run Codex remediation",
    );
    let mut errors = Vec::new();
    validate_agent_plugin_workflow_text(
        "dependency-remediation.yml",
        &head_owned_config,
        AgentPluginWorkflowKind::Marketplace,
        &mut errors,
    );
    assert!(
        errors
            .iter()
            .any(|error| error.contains("must not use requested-head Codex inputs")),
        "{errors:?}"
    );

    let job_scoped_token = dependency.replace(
        "    runs-on: ubuntu-latest\n",
        "    runs-on: ubuntu-latest\n    env:\n      GH_TOKEN: ${{ github.token }}\n",
    );
    let mut errors = Vec::new();
    validate_agent_plugin_workflow_text(
        "dependency-remediation.yml",
        &job_scoped_token,
        AgentPluginWorkflowKind::Marketplace,
        &mut errors,
    );
    assert!(
        errors
            .iter()
            .any(|error| error.contains("job-level environment")),
        "{errors:?}"
    );

    let execution_state = safe_execution_state_workflow();
    let mut errors = Vec::new();
    validate_agent_plugin_workflow_text(
        "execution_state_sync.yml",
        execution_state,
        AgentPluginWorkflowKind::ExecutionState,
        &mut errors,
    );
    assert_eq!(errors, Vec::<String>::new());

    let missing_execution_diagnostic = execution_state.replace(
        "jq -n '{}' > \"$root/run-context.json\"",
        "jq -n '{}' > \"$root/context.json\"",
    );
    let mut errors = Vec::new();
    validate_agent_plugin_workflow_text(
        "execution_state_sync.yml",
        &missing_execution_diagnostic,
        AgentPluginWorkflowKind::ExecutionState,
        &mut errors,
    );
    assert!(
        errors
            .iter()
            .any(|error| error.contains("run-context diagnostic")),
        "{errors:?}"
    );

    let unsafe_execution_state = execution_state
        .replace("pull_request.base.sha", "pull_request.head.sha")
        .replace(
            "    if: github.event_name != 'pull_request_target' || github.event.pull_request.head.repo.full_name == github.repository\n",
            "",
        );
    let mut errors = Vec::new();
    validate_agent_plugin_workflow_text(
        "execution_state_sync.yml",
        &unsafe_execution_state,
        AgentPluginWorkflowKind::ExecutionState,
        &mut errors,
    );
    assert!(
        errors.iter().any(|error| error.contains("reject fork")),
        "{errors:?}"
    );
    assert!(
        errors.iter().any(|error| error.contains("head content")),
        "{errors:?}"
    );

    let stale_closed_execution_state = execution_state.replace(
        "github.event.action != 'closed' && github.event.pull_request.base.sha || github.sha",
        "github.event.pull_request.base.sha || github.sha",
    );
    let mut errors = Vec::new();
    validate_agent_plugin_workflow_text(
        "execution_state_sync.yml",
        &stale_closed_execution_state,
        AgentPluginWorkflowKind::ExecutionState,
        &mut errors,
    );
    assert!(
        errors
            .iter()
            .any(|error| error.contains("current base revision for closed events")),
        "{errors:?}"
    );

    let untrusted_event = execution_state.replace("pull_request_target", "pull_request");
    let mut errors = Vec::new();
    validate_agent_plugin_workflow_text(
        "execution_state_sync.yml",
        &untrusted_event,
        AgentPluginWorkflowKind::ExecutionState,
        &mut errors,
    );
    assert!(
        errors
            .iter()
            .any(|error| error.contains("trusted pull_request_target event")),
        "{errors:?}"
    );

    let aliased_execution_token = execution_state.replace(
        "      ROADMAP_GH_TOKEN_SOURCE:",
        "      ROADMAP_GH_TOKEN: ${{ secrets.GH_PROJECTS_TOKEN }}\n      ROADMAP_GH_TOKEN_SOURCE:",
    );
    let mut errors = Vec::new();
    validate_agent_plugin_workflow_text(
        "execution_state_sync.yml",
        &aliased_execution_token,
        AgentPluginWorkflowKind::ExecutionState,
        &mut errors,
    );
    assert!(
        errors
            .iter()
            .any(|error| error.contains("must not retain the project PAT")),
        "{errors:?}"
    );

    let unrelated_project_token = execution_state.replace(
        "      - name: Snapshot project",
        "      - name: Leaky helper\n        env:\n          PROJECT_PAT: ${{ secrets.GH_PROJECTS_TOKEN }}\n        run: echo unsafe\n      - name: Snapshot project",
    );
    let mut errors = Vec::new();
    validate_agent_plugin_workflow_text(
        "execution_state_sync.yml",
        &unrelated_project_token,
        AgentPluginWorkflowKind::ExecutionState,
        &mut errors,
    );
    assert!(
        errors
            .iter()
            .any(|error| error.contains("only to publisher GitHub operation steps")),
        "{errors:?}"
    );
}
