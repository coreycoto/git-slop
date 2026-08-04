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
            "interpret-results",
            "plan-maintenance",
            "run-report",
        ]
        .into_iter()
        .collect()
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
