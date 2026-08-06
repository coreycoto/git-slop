use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use tempfile::TempDir;

use super::*;

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
        include_str!("../../../plugins/git-slop/plugin.json"),
    );

    let mut errors = Vec::new();
    validate_product_plugin(temp.path(), &mut errors);

    assert_eq!(errors, Vec::<String>::new());
}

#[test]
fn portable_agent_plugin_version_matches_cli_version() {
    let temp = TempDir::new().unwrap();
    let mut manifest: JsonValue =
        serde_json::from_str(include_str!("../../../plugins/git-slop/plugin.json")).unwrap();
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
        include_str!("../../../plugins/git-slop/plugin.json"),
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
        include_str!("../../../plugins/git-slop/plugin.json"),
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
        serde_json::from_str(include_str!("../../../plugins/git-slop/plugin.json")).unwrap();
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

#[test]
fn portable_agent_plugin_rejects_missing_codex_extension_and_asset() {
    let temp = TempDir::new().unwrap();
    let mut manifest: JsonValue =
        serde_json::from_str(include_str!("../../../plugins/git-slop/plugin.json")).unwrap();
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
        include_str!("../../../plugins/git-slop/plugin.json"),
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
        include_str!("../../../plugins/git-slop/plugin.json"),
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
        include_str!("../../../plugins/git-slop/plugin.json"),
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
        include_str!("../../../plugins/git-slop/plugin.json"),
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
        include_str!("../../../plugins/git-slop/plugin.json"),
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
    fs::write(root.join("Cargo.toml"), include_str!("../../../Cargo.toml")).unwrap();
    let plugin_root = root.join(GIT_SLOP_PLUGIN_ROOT);
    fs::create_dir_all(plugin_root.join("assets")).unwrap();
    fs::create_dir_all(plugin_root.join(".codex-plugin")).unwrap();
    fs::write(plugin_root.join("plugin.json"), manifest).unwrap();
    fs::write(
        plugin_root.join(".codex-plugin/plugin.json"),
        include_str!("../../../plugins/git-slop/.codex-plugin/plugin.json"),
    )
    .unwrap();
    let icon = include_str!("../../../plugins/git-slop/assets/git-slop.svg");
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
                include_str!("../../../plugins/git-slop/skills/adopt-repo/agents/openai.yaml")
            }
            "install-update" => {
                include_str!("../../../plugins/git-slop/skills/install-update/agents/openai.yaml")
            }
            "review-results" => {
                include_str!("../../../plugins/git-slop/skills/review-results/agents/openai.yaml")
            }
            "run-report" => {
                include_str!("../../../plugins/git-slop/skills/run-report/agents/openai.yaml")
            }
            _ => unreachable!("unexpected git-slop skill fixture: {skill}"),
        };
        fs::write(skill_root.join("agents/openai.yaml"), openai_yaml).unwrap();
        fs::write(skill_root.join("assets/git-slop.svg"), icon).unwrap();
    }
}
