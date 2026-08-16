#[test]
fn repository_codex_surface_passes() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    assert_eq!(validate(root, false), Vec::<String>::new());
}

#[test]
fn contract_inventory_is_stable() {
    assert_eq!(
        AGENTS.iter().map(|agent| agent.name).collect::<Vec<_>>(),
        [
            "dependency_patcher",
            "docs_taxonomist",
            "governance_auditor",
            "merge_gatekeeper",
            "release_publisher",
        ]
    );
    assert_eq!(
        WORKFLOWS
            .iter()
            .map(|workflow| workflow.name)
            .collect::<Vec<_>>(),
        [
            "dependency-remediation.yml",
            "docs-taxonomy.yml",
            "governance-reconcile.yml",
            "merge-on-green.yml",
            "release-publish.yml",
        ]
    );
    let runtime_workflow_names = WORKFLOWS
        .iter()
        .filter(|workflow| workflow.uses_agent_plugins)
        .map(|workflow| workflow.name)
        .chain(std::iter::once("execution_state_sync.yml"))
        .collect::<Vec<_>>();
    assert_eq!(
        runtime_workflow_names,
        [
            "dependency-remediation.yml",
            "docs-taxonomy.yml",
            "governance-reconcile.yml",
            "merge-on-green.yml",
            "execution_state_sync.yml",
        ]
    );
    assert_eq!(
        GIT_SLOP_PLUGIN_SKILLS.into_iter().collect::<BTreeSet<_>>(),
        [
            "adopt-repo",
            "install-update",
            "review-results",
            "run-report",
        ]
        .into_iter()
        .collect()
    );
    assert_eq!(
        GIT_SLOP_PLUGIN_CLIENTS,
        [
            "ChatGPT & Codex",
            "VS Code",
            "Cursor",
            "GitHub Copilot",
            "Kiro",
        ]
    );
}

#[test]
fn portable_agent_plugin_contract_passes() {
    let temp = TempDir::new().unwrap();
    write_product_plugin_fixture(
        temp.path(),
        include_str!("../../../../plugins/git-slop/plugin.json"),
    );

    let mut errors = Vec::new();
    validate_product_plugin(temp.path(), &mut errors);

    assert_eq!(errors, Vec::<String>::new());
}

#[test]
fn portable_agent_plugin_version_matches_cli_version() {
    let temp = TempDir::new().unwrap();
    let mut manifest: JsonValue =
        serde_json::from_str(include_str!("../../../../plugins/git-slop/plugin.json")).unwrap();
    manifest["version"] = json!("0.9.5");
    write_product_plugin_fixture(
        temp.path(),
        &serde_json::to_string_pretty(&manifest).unwrap(),
    );
    let compatibility_path = temp.path().join(GIT_SLOP_CODEX_COMPAT_MANIFEST);
    let mut compatibility_manifest: JsonValue =
        serde_json::from_str(&fs::read_to_string(&compatibility_path).unwrap()).unwrap();
    compatibility_manifest["version"] = json!("0.9.5");
    fs::write(
        compatibility_path,
        serde_json::to_string_pretty(&compatibility_manifest).unwrap(),
    )
    .unwrap();

    let mut errors = Vec::new();
    validate_product_plugin(temp.path(), &mut errors);

    assert!(
        errors
            .iter()
            .any(|error| error.contains("must match Cargo.toml package.version"))
    );
}

#[test]
fn portable_agent_plugin_requires_codex_compatibility_overlay() {
    let temp = TempDir::new().unwrap();
    write_product_plugin_fixture(
        temp.path(),
        include_str!("../../../../plugins/git-slop/plugin.json"),
    );
    fs::remove_file(temp.path().join(GIT_SLOP_CODEX_COMPAT_MANIFEST)).unwrap();

    let mut errors = Vec::new();
    validate_product_plugin(temp.path(), &mut errors);

    assert!(
        errors
            .iter()
            .any(|error| error.contains(".codex-plugin/plugin.json is missing"))
    );
}

#[test]
fn portable_agent_plugin_rejects_codex_compatibility_overlay_drift() {
    let temp = TempDir::new().unwrap();
    write_product_plugin_fixture(
        temp.path(),
        include_str!("../../../../plugins/git-slop/plugin.json"),
    );
    let compatibility_path = temp.path().join(GIT_SLOP_CODEX_COMPAT_MANIFEST);
    let mut compatibility_manifest: JsonValue =
        serde_json::from_str(&fs::read_to_string(&compatibility_path).unwrap()).unwrap();
    compatibility_manifest["version"] = json!("0.2.9");
    compatibility_manifest["skills"] = json!("./skills");
    fs::write(
        compatibility_path,
        serde_json::to_string_pretty(&compatibility_manifest).unwrap(),
    )
    .unwrap();

    let mut errors = Vec::new();
    validate_product_plugin(temp.path(), &mut errors);

    assert!(
        errors
            .iter()
            .any(|error| error.contains("exact metadata-only mirror"))
    );
}

#[test]
fn portable_agent_plugin_rejects_wrong_schema_and_legacy_fields() {
    let temp = TempDir::new().unwrap();
    let mut manifest: JsonValue =
        serde_json::from_str(include_str!("../../../../plugins/git-slop/plugin.json")).unwrap();
    manifest["$schema"] = json!("https://example.com/plugin.schema.json");
    manifest["skills"] = json!("./skills/");
    write_product_plugin_fixture(
        temp.path(),
        &serde_json::to_string_pretty(&manifest).unwrap(),
    );

    let mut errors = Vec::new();
    validate_product_plugin(temp.path(), &mut errors);

    assert!(
        errors
            .iter()
            .any(|error| error.contains("portable Agent Plugins fields"))
    );
    assert!(
        errors
            .iter()
            .any(|error| error.contains(AGENT_PLUGIN_SCHEMA))
    );
}
