#[test]
fn runtime_release_manifest_requires_canonical_immutable_pins() {
    let manifest = valid_marketplace_source_manifest();
    let mut errors = Vec::new();
    validate_marketplace_source_manifest(&manifest, &mut errors);
    assert_eq!(errors, Vec::<String>::new());

    let mut malformed = manifest.clone();
    malformed["ref"] = json!("ABCDEF");
    malformed["runtime_release"]["sha256"] = json!("not-a-digest");
    malformed["runtime_release"]["size"] = json!(0);
    malformed["runtime_release"]["target"] = json!("aarch64-unknown-linux-gnu");
    malformed["runtime_release"]["member"] = json!("../agent-plugins");
    malformed["runtime_release"]["unexpected"] = json!(true);

    let mut errors = Vec::new();
    validate_marketplace_source_manifest(&malformed, &mut errors);
    for expected in [
        "40-character source revision",
        "64-character SHA-256",
        "positive archive byte count",
        "runtime_release.target",
        "runtime_release.member",
        "exactly the canonical fields",
    ] {
        assert!(
            errors.iter().any(|error| error.contains(expected)),
            "missing {expected:?} from {errors:?}"
        );
    }

    let mut substituted = manifest;
    substituted["runtime_release"]["sha256"] = json!("b".repeat(64));
    substituted["runtime_release"]["size"] = json!(EXPECTED_RUNTIME_SIZE + 1);
    let mut errors = Vec::new();
    validate_marketplace_source_manifest(&substituted, &mut errors);
    for expected in [
        "expected v0.1.0 archive SHA-256",
        "expected v0.1.0 archive byte count",
    ] {
        assert!(
            errors.iter().any(|error| error.contains(expected)),
            "missing {expected:?} from {errors:?}"
        );
    }
}

#[test]
fn wrapper_contract_rejects_implicit_or_legacy_acquisition() {
    let wrapper = valid_wrapper_fixture();
    let mut errors = Vec::new();
    validate_agent_plugin_wrapper_text("wrapper", wrapper, &mut errors);
    assert_eq!(errors, Vec::<String>::new());

    let implicit = wrapper.replace(
        "verify_install\n    exec_runtime \"$@\"",
        "verify_install\n    download_release_assets\n    exec_runtime \"$@\"",
    );
    let mut errors = Vec::new();
    validate_agent_plugin_wrapper_text("wrapper", &implicit, &mut errors);
    assert!(errors.iter().any(|error| {
        error.contains("download_release_assets only in its definition and prepare_runtime")
    }));

    let legacy = format!("{wrapper}\nuv run python -m pip install agent-plugins\n");
    let mut errors = Vec::new();
    validate_agent_plugin_wrapper_text("wrapper", &legacy, &mut errors);
    assert!(errors.iter().any(|error| error.contains("path uv ")));
    assert!(errors.iter().any(|error| error.contains("path pip ")));

    let wrong_lock_format = wrapper.replace(
        "pex-lock-from-hashed-requirements",
        "source-requirements-only",
    );
    let mut errors = Vec::new();
    validate_agent_plugin_wrapper_text("wrapper", &wrong_lock_format, &mut errors);
    assert!(
        errors
            .iter()
            .any(|error| error.contains("dependency lock format"))
    );

    let hardened_interpreter = wrapper.replace(
        "  unset AGENT_PLUGINS_READ_TOKEN GH_TOKEN GITHUB_TOKEN\n  exec env PEX_INTERPRETER=1 \"$runtime_executable\" \"$@\"",
        "  clean_environment=(env -i)\n  exec \"${clean_environment[@]}\" PEX_INTERPRETER=1 \"$runtime_executable\" \"$@\"",
    );
    let mut errors = Vec::new();
    validate_agent_plugin_wrapper_text("wrapper", &hardened_interpreter, &mut errors);
    assert_eq!(errors, Vec::<String>::new());

    let leaking_interpreter = wrapper.replace(
        "unset AGENT_PLUGINS_READ_TOKEN GH_TOKEN GITHUB_TOKEN",
        "unset AGENT_PLUGINS_READ_TOKEN",
    );
    let mut errors = Vec::new();
    validate_agent_plugin_wrapper_text("wrapper", &leaking_interpreter, &mut errors);
    assert!(
        errors
            .iter()
            .any(|error| error.contains("verified PEX interpreter"))
    );
}

#[test]
fn runtime_workflow_rejects_misplaced_token_old_setup_and_cache() {
    let workflow = safe_marketplace_workflow();
    let mut errors = Vec::new();
    validate_agent_plugin_workflow_text(
        "docs-taxonomy.yml",
        workflow,
        AgentPluginWorkflowKind::Marketplace,
        &mut errors,
    );
    assert_eq!(errors, Vec::<String>::new());

    let missing_diagnostic = workflow.replace(
        "      - name: Prepare artifact roots\n        if: steps.codex_preflight.outputs.enabled == 'true'\n        run: |\n          mkdir -p .artifacts/codex .artifacts/docs-taxonomy\n          jq -n '{}' > .artifacts/docs-taxonomy/run-context.json\n",
        "",
    );
    let mut errors = Vec::new();
    validate_agent_plugin_workflow_text(
        "docs-taxonomy.yml",
        &missing_diagnostic,
        AgentPluginWorkflowKind::Marketplace,
        &mut errors,
    );
    assert!(
        errors
            .iter()
            .any(|error| error.contains("run-context diagnostic")),
        "{errors:?}"
    );

    let misplaced = workflow.replace(
        "      - name: Verify runtime\n        run:",
        "      - name: Verify runtime\n        env:\n          LEAKED_TOKEN: ${{ secrets.AGENT_PLUGINS_READ_TOKEN }}\n        run:",
    );
    let mut errors = Vec::new();
    validate_agent_plugin_workflow_text(
        "docs-taxonomy.yml",
        &misplaced,
        AgentPluginWorkflowKind::Marketplace,
        &mut errors,
    );
    assert!(
        errors
            .iter()
            .any(|error| error.contains("only in the dedicated"))
    );

    let expanded_acquisition = workflow.replace(
        "        run: scripts/with-agent-plugins.sh --prepare",
        "        run: |\n          scripts/with-agent-plugins.sh --prepare\n          env",
    );
    let mut errors = Vec::new();
    validate_agent_plugin_workflow_text(
        "docs-taxonomy.yml",
        &expanded_acquisition,
        AgentPluginWorkflowKind::Marketplace,
        &mut errors,
    );
    assert!(
        errors
            .iter()
            .any(|error| error.contains("acquisition step must run only"))
    );

    let old_setup = workflow.replace(
        "      - name: Acquire runtime",
        "      - uses: actions/setup-python@v6\n      - name: Acquire runtime",
    );
    let mut errors = Vec::new();
    validate_agent_plugin_workflow_text(
        "docs-taxonomy.yml",
        &old_setup,
        AgentPluginWorkflowKind::Marketplace,
        &mut errors,
    );
    assert!(
        errors
            .iter()
            .any(|error| error.contains("actions/setup-python"))
    );

    let cached = workflow.replace(
        "      - name: Acquire runtime",
        "      - uses: actions/cache@v5\n      - name: Acquire runtime",
    );
    let mut errors = Vec::new();
    validate_agent_plugin_workflow_text(
        "docs-taxonomy.yml",
        &cached,
        AgentPluginWorkflowKind::Marketplace,
        &mut errors,
    );
    assert!(errors.iter().any(|error| error.contains("actions/cache@")));
}
