#[test]
fn portable_agent_plugin_rejects_missing_codex_extension_and_asset() {
    let temp = TempDir::new().unwrap();
    let mut manifest: JsonValue =
        serde_json::from_str(include_str!("../../../../plugins/git-slop/plugin.json")).unwrap();
    manifest["extensions"] = json!({});
    write_product_plugin_fixture(
        temp.path(),
        &serde_json::to_string_pretty(&manifest).unwrap(),
    );
    fs::remove_file(temp.path().join("plugins/git-slop/assets/git-slop.svg")).unwrap();

    let mut errors = Vec::new();
    validate_product_plugin(temp.path(), &mut errors);

    assert!(
        errors
            .iter()
            .any(|error| error.contains("extensions.com.openai"))
    );
    assert!(
        errors
            .iter()
            .any(|error| error.contains("only the com.openai"))
    );
    assert!(
        errors
            .iter()
            .any(|error| error.contains("referenced asset is missing"))
    );
}

#[test]
fn portable_agent_plugin_rejects_invalid_skill_frontmatter() {
    let temp = TempDir::new().unwrap();
    write_product_plugin_fixture(
        temp.path(),
        include_str!("../../../../plugins/git-slop/plugin.json"),
    );
    fs::write(
        temp.path()
            .join("plugins/git-slop/skills/adopt-repo/SKILL.md"),
        "---\nname: wrong-name\ndescription: ''\n---\n",
    )
    .unwrap();

    let mut errors = Vec::new();
    validate_product_plugin(temp.path(), &mut errors);

    assert!(errors.iter().any(|error| error.contains("name must match")));
    assert!(
        errors
            .iter()
            .any(|error| error.contains("non-empty description"))
    );
}

#[test]
fn portable_agent_plugin_rejects_skill_presentation_and_icon_drift() {
    let temp = TempDir::new().unwrap();
    write_product_plugin_fixture(
        temp.path(),
        include_str!("../../../../plugins/git-slop/plugin.json"),
    );
    let skill_root = temp.path().join("plugins/git-slop/skills/adopt-repo");
    fs::write(
        skill_root.join("agents/openai.yaml"),
        "interface:\n  display_name: Wrong\n",
    )
    .unwrap();
    fs::write(skill_root.join("assets/git-slop.svg"), "<svg>wrong</svg>").unwrap();
    fs::write(skill_root.join("agents/cursor.yaml"), "interface: {}\n").unwrap();

    let mut errors = Vec::new();
    validate_product_plugin(temp.path(), &mut errors);

    assert!(
        errors
            .iter()
            .any(|error| error.contains("expected OpenAI presentation contract"))
    );
    assert!(
        errors
            .iter()
            .any(|error| error.contains("must be the same Git Slop icon"))
    );
    assert!(
        errors
            .iter()
            .any(|error| error.contains("must contain only the optional OpenAI presentation"))
    );
}

#[test]
fn portable_agent_plugin_requires_implicit_skill_invocation() {
    let temp = TempDir::new().unwrap();
    write_product_plugin_fixture(
        temp.path(),
        include_str!("../../../../plugins/git-slop/plugin.json"),
    );
    let openai_path = temp
        .path()
        .join("plugins/git-slop/skills/install-update/agents/openai.yaml");
    let openai_yaml = fs::read_to_string(&openai_path).unwrap().replace(
        "allow_implicit_invocation: true",
        "allow_implicit_invocation: false",
    );
    fs::write(openai_path, openai_yaml).unwrap();

    let mut errors = Vec::new();
    validate_product_plugin(temp.path(), &mut errors);

    assert!(
        errors
            .iter()
            .any(|error| { error.contains("policy.allow_implicit_invocation set to true") })
    );
}

#[test]
fn portable_agent_plugin_rejects_implicit_trigger_description_drift() {
    let temp = TempDir::new().unwrap();
    write_product_plugin_fixture(
        temp.path(),
        include_str!("../../../../plugins/git-slop/plugin.json"),
    );
    fs::write(
        temp.path()
            .join("plugins/git-slop/skills/run-report/SKILL.md"),
        "---\nname: run-report\ndescription: Do Git Slop things.\n---\n",
    )
    .unwrap();

    let mut errors = Vec::new();
    validate_product_plugin(temp.path(), &mut errors);

    assert!(errors.iter().any(|error| {
        error.contains("must preserve its expected implicit-invocation trigger description")
    }));
}

#[test]
fn portable_agent_plugin_rejects_undersized_or_fixed_color_brand_icon() {
    let temp = TempDir::new().unwrap();
    write_product_plugin_fixture(
        temp.path(),
        include_str!("../../../../plugins/git-slop/plugin.json"),
    );
    let icon_path = temp.path().join(GIT_SLOP_ICON);
    let icon = fs::read_to_string(&icon_path)
        .unwrap()
        .replace(r#"width="64" height="64""#, r#"width="24" height="24""#)
        .replace(" role=", r##" color="#24292f" role=""##);
    fs::write(icon_path, icon).unwrap();

    let mut errors = Vec::new();
    validate_product_plugin(temp.path(), &mut errors);

    assert!(errors.iter().any(|error| {
        error.contains("must declare 64x64 dimensions and preserve viewBox 0 0 24 24")
    }));
    assert!(
        errors
            .iter()
            .any(|error| error.contains("must inherit its foreground color"))
    );
}

#[test]
fn execpolicy_parser_accepts_current_prompt_output() {
    let output = r#"{
        "matchedRules": [{
            "prefixRuleMatch": {
                "matchedPrefix": ["git", "push"],
                "decision": "prompt",
                "justification": "Publishing changes is allowed with explicit approval."
            }
        }],
        "decision": "prompt"
    }"#;

    assert_eq!(parse_execpolicy_decision(output).unwrap(), "prompt");
    assert_eq!(require_prompt_decision(output), Ok(()));
}

#[test]
fn execpolicy_parser_rejects_non_prompt_decisions() {
    let error = require_prompt_decision(r#"{"decision":"allow"}"#).unwrap_err();
    assert!(error.contains("decision was \"allow\"; expected \"prompt\""));

    let error = require_prompt_decision(r#"{"decision":"forbidden"}"#).unwrap_err();
    assert!(error.contains("decision was \"forbidden\"; expected \"prompt\""));
}
