#[test]
fn execpolicy_parser_rejects_missing_or_malformed_decisions() {
    assert!(
        require_prompt_decision("")
            .unwrap_err()
            .contains("stdout was empty")
    );
    assert!(
        require_prompt_decision("not-json")
            .unwrap_err()
            .contains("stdout was not valid JSON")
    );
    assert!(
        require_prompt_decision(r#"{"matchedRules":[]}"#)
            .unwrap_err()
            .contains("did not contain a string decision")
    );
}

#[test]
fn config_validation_rejects_legacy_profiles() {
    let temp = TempDir::new().unwrap();
    let codex_root = temp.path().join(".codex");
    fs::create_dir_all(&codex_root).unwrap();
    fs::write(
        codex_root.join("config.toml"),
        "approval_policy = \"on-request\"\n\
         sandbox_mode = \"workspace-write\"\n\
         profile = \"ci_mutation\"\n\
         [profiles.ci_mutation]\n\
         approval_policy = \"never\"\n",
    )
    .unwrap();
    write_profiles(&codex_root);

    let mut errors = Vec::new();
    validate_codex_config(temp.path(), &mut errors);

    assert!(
        errors
            .iter()
            .any(|error| error.contains("legacy profile selector"))
    );
    assert!(
        errors
            .iter()
            .any(|error| error.contains("legacy [profiles.*] tables"))
    );
}

#[test]
fn config_validation_requires_safe_standalone_profiles() {
    let temp = TempDir::new().unwrap();
    let codex_root = temp.path().join(".codex");
    fs::create_dir_all(&codex_root).unwrap();
    fs::write(
        codex_root.join("config.toml"),
        "approval_policy = \"on-request\"\nsandbox_mode = \"workspace-write\"\n",
    )
    .unwrap();
    fs::write(
        codex_root.join("ci_readonly.config.toml"),
        "approval_policy = \"on-request\"\nsandbox_mode = \"workspace-write\"\n",
    )
    .unwrap();
    fs::write(
        codex_root.join("ci_mutation.config.toml"),
        "approval_policy = \"never\"\nsandbox_mode = \"workspace-write\"\n",
    )
    .unwrap();

    let mut errors = Vec::new();
    validate_codex_config(temp.path(), &mut errors);

    assert!(errors.iter().any(|error| {
        error.contains("ci_readonly.config.toml must set top-level approval_policy to never")
    }));
    assert!(errors.iter().any(|error| {
        error.contains("ci_readonly.config.toml must set top-level sandbox_mode to read-only")
    }));
    assert!(
        errors
            .iter()
            .any(|error| error.contains(".codex/ci_release.config.toml is missing"))
    );
}

fn write_profiles(codex_root: &Path) {
    for (name, sandbox_mode) in CI_PROFILES {
        fs::write(
            codex_root.join(format!("{name}.config.toml")),
            format!("approval_policy = \"never\"\nsandbox_mode = \"{sandbox_mode}\"\n"),
        )
        .unwrap();
    }
}

fn write_product_plugin_fixture(root: &Path, manifest: &str) {
    fs::write(root.join("Cargo.toml"), include_str!("../../../../Cargo.toml")).unwrap();
    let plugin_root = root.join(GIT_SLOP_PLUGIN_ROOT);
    fs::create_dir_all(plugin_root.join("assets")).unwrap();
    fs::create_dir_all(plugin_root.join(".codex-plugin")).unwrap();
    fs::write(plugin_root.join("plugin.json"), manifest).unwrap();
    fs::write(
        plugin_root.join(".codex-plugin/plugin.json"),
        include_str!("../../../../plugins/git-slop/.codex-plugin/plugin.json"),
    )
    .unwrap();
    let icon = include_str!("../../../../plugins/git-slop/assets/git-slop.svg");
    fs::write(plugin_root.join("assets/git-slop.svg"), icon).unwrap();
    for skill in GIT_SLOP_PLUGIN_SKILLS {
        let skill_root = plugin_root.join("skills").join(skill);
        fs::create_dir_all(skill_root.join("agents")).unwrap();
        fs::create_dir_all(skill_root.join("assets")).unwrap();
        let trigger_description = GIT_SLOP_SKILL_CONTRACTS
            .iter()
            .find(|presentation| presentation.skill_name == skill)
            .unwrap()
            .trigger_description;
        fs::write(
            skill_root.join("SKILL.md"),
            format!("---\nname: {skill}\ndescription: {trigger_description}\n---\n"),
        )
        .unwrap();
        let openai_yaml = match skill {
            "adopt-repo" => {
                include_str!("../../../../plugins/git-slop/skills/adopt-repo/agents/openai.yaml")
            }
            "install-update" => {
                include_str!("../../../../plugins/git-slop/skills/install-update/agents/openai.yaml")
            }
            "review-results" => {
                include_str!("../../../../plugins/git-slop/skills/review-results/agents/openai.yaml")
            }
            "run-report" => {
                include_str!("../../../../plugins/git-slop/skills/run-report/agents/openai.yaml")
            }
            _ => unreachable!("unexpected git-slop skill fixture: {skill}"),
        };
        fs::write(skill_root.join("agents/openai.yaml"), openai_yaml).unwrap();
        fs::write(skill_root.join("assets/git-slop.svg"), icon).unwrap();
    }
}
