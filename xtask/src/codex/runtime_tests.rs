use std::fs;

use serde_json::{Value as JsonValue, json};
use tempfile::TempDir;

use super::runtime_manifest::{
    CODEX_CLI_VERSION, GH_STEWARD_REPOSITORY, GH_STEWARD_VERSION, PUBLIC_PLUGIN_MARKETPLACE,
    PUBLIC_PLUGIN_NAMES, PUBLIC_PLUGIN_SOURCE, PUBLIC_PLUGIN_SOURCE_SHA,
    validate_gh_steward_lock_manifest, validate_marketplace_source_manifest,
};
use super::runtime_workflows::{AgentPluginWorkflowKind, validate_agent_plugin_workflow_text};
use super::{EXPECTED_PLUGIN_URL, validate_release_workflow};

#[test]
fn public_plugin_source_requires_the_exact_minimal_source_lock() {
    let manifest = json!({
        "schema_version": 1,
        "repository": PUBLIC_PLUGIN_SOURCE,
        "ref": PUBLIC_PLUGIN_SOURCE_SHA,
        "marketplace_name": PUBLIC_PLUGIN_MARKETPLACE,
        "plugins": PUBLIC_PLUGIN_NAMES,
        "codex_cli_version": CODEX_CLI_VERSION,
    });
    let mut errors = Vec::new();
    validate_marketplace_source_manifest(&manifest, &mut errors);
    assert!(errors.is_empty(), "{errors:?}");
    assert_eq!(
        EXPECTED_PLUGIN_URL,
        "https://github.com/coreycoto/agent-plugins.git"
    );

    let mut changed = manifest.clone();
    changed["repository"] = json!("coreycoto/agent-plugins-private-history");
    changed["plugins"] = json!(["project-management"]);
    changed["unexpected"] = json!(true);
    let mut errors = Vec::new();
    validate_marketplace_source_manifest(&changed, &mut errors);
    assert!(
        errors.iter().any(|error| error.contains("repository")),
        "{errors:?}"
    );
    assert!(
        errors
            .iter()
            .any(|error| error.contains("project-management and product-development in order")),
        "{errors:?}"
    );
    assert!(
        errors
            .iter()
            .any(|error| error.contains("exactly the public")),
        "{errors:?}"
    );
}

#[test]
fn gh_steward_lock_requires_one_clean_exact_version_and_four_valid_asset_pins() {
    let lock = valid_gh_steward_lock();
    let mut errors = Vec::new();
    validate_gh_steward_lock_manifest(&lock, &mut errors);
    assert!(errors.is_empty(), "{errors:?}");

    let mut unqualified = lock;
    unqualified["source_revision"] = json!("UNQUALIFIED_SOURCE_REVISION");
    unqualified["asset_sha256"]["linux/arm64"] = json!("UNQUALIFIED_DIGEST");
    unqualified["asset_sha256"]
        .as_object_mut()
        .unwrap()
        .remove("darwin/amd64");
    let mut errors = Vec::new();
    validate_gh_steward_lock_manifest(&unqualified, &mut errors);
    assert!(
        errors.iter().any(|error| error.contains("source commit")),
        "{errors:?}"
    );
    assert!(
        errors.iter().any(|error| error.contains("all four")),
        "{errors:?}"
    );
}

#[test]
fn gh_steward_lock_rejects_extra_fields_and_wrong_tool_identity() {
    let mut lock = valid_gh_steward_lock();
    lock["repository"] = json!("someone/else");
    lock["version"] = json!("0.2.0");
    lock["extra"] = json!(false);
    let mut errors = Vec::new();
    validate_gh_steward_lock_manifest(&lock, &mut errors);
    assert!(
        errors
            .iter()
            .any(|error| error.contains("exactly the versioned")),
        "{errors:?}"
    );
    assert!(
        errors.iter().any(|error| error.contains("repository")),
        "{errors:?}"
    );
    assert!(
        errors.iter().any(|error| error.contains("version")),
        "{errors:?}"
    );
}

#[test]
fn codex_workflow_requires_verified_native_acquisition_and_isolated_plugins() {
    let good = safe_codex_workflow();
    let mut errors = Vec::new();
    validate_agent_plugin_workflow_text(
        "fixture.yml",
        good,
        AgentPluginWorkflowKind::CodexPlugins,
        &mut errors,
    );
    assert!(errors.is_empty(), "{errors:?}");

    for (changed, expected) in [
        (
            good.replace(
                "scripts/with-gh-steward.sh --verify",
                "scripts/with-gh-steward.sh --prepare",
            ),
            "exactly one native acquisition and verification pair",
        ),
        (
            good.replace(
                "scripts/prepare-codex-plugins.sh",
                "scripts/old-plugin-runtime.sh",
            ),
            "install the selected public plugins",
        ),
        (
            good.replace("persist-credentials: false", "persist-credentials: true"),
            "persisted credentials disabled",
        ),
        (good.replace("gpt-6-luna", "gpt-6-astra"), "gpt-6-luna"),
    ] {
        let mut errors = Vec::new();
        validate_agent_plugin_workflow_text(
            "fixture.yml",
            &changed,
            AgentPluginWorkflowKind::CodexPlugins,
            &mut errors,
        );
        assert!(
            errors.iter().any(|error| error.contains(expected)),
            "{expected}: {errors:?}"
        );
    }

    for leaked in [
        "AGENT_PLUGINS_READ_TOKEN: ${{ secrets.AGENT_PLUGINS_READ_TOKEN }}",
        "actions/cache@v5",
        "agent-plugins-private-history",
    ] {
        let mut errors = Vec::new();
        validate_agent_plugin_workflow_text(
            "fixture.yml",
            &good.replace("# additional trusted annotation", leaked),
            AgentPluginWorkflowKind::CodexPlugins,
            &mut errors,
        );
        assert!(!errors.is_empty(), "workflow accepted {leaked}");
    }
    let leaked_acquisition_token = good.replace(
        "      - name: Acquire gh-steward\n        run:",
        "      - name: Acquire gh-steward\n        env:\n          GH_TOKEN: ${{ github.token }}\n        run:",
    );
    let mut errors = Vec::new();
    validate_agent_plugin_workflow_text(
        "fixture.yml",
        &leaked_acquisition_token,
        AgentPluginWorkflowKind::CodexPlugins,
        &mut errors,
    );
    assert!(
        errors
            .iter()
            .any(|error| error.contains("must not receive GitHub tokens")),
        "{errors:?}"
    );
}

#[test]
fn execution_workflow_keeps_native_recovery_and_trusted_apply_handoff() {
    let good = include_str!("../../../.github/workflows/execution_state_sync.yml");
    let mut errors = Vec::new();
    validate_agent_plugin_workflow_text(
        "execution_state_sync.yml",
        good,
        AgentPluginWorkflowKind::ExecutionState,
        &mut errors,
    );
    assert!(errors.is_empty(), "{errors:?}");

    let untrusted_checkout =
        good.replacen("persist-credentials: false", "persist-credentials: true", 1);
    let mut errors = Vec::new();
    validate_agent_plugin_workflow_text(
        "execution_state_sync.yml",
        &untrusted_checkout,
        AgentPluginWorkflowKind::ExecutionState,
        &mut errors,
    );
    assert!(
        errors
            .iter()
            .any(|error| error.contains("exact trusted workflow source")),
        "{errors:?}"
    );

    let unbound_handoff = good.replace("runs acquire-handoff", "runs fetch-handoff");
    let mut errors = Vec::new();
    validate_agent_plugin_workflow_text(
        "execution_state_sync.yml",
        &unbound_handoff,
        AgentPluginWorkflowKind::ExecutionState,
        &mut errors,
    );
    assert!(
        errors
            .iter()
            .any(|error| error.contains("immutable native handoff")),
        "{errors:?}"
    );

    let unbound_restored_journal =
        good.replace("          [[ -s \"$root/recovery-source.json\" ]]\n", "");
    let mut errors = Vec::new();
    validate_agent_plugin_workflow_text(
        "execution_state_sync.yml",
        &unbound_restored_journal,
        AgentPluginWorkflowKind::ExecutionState,
        &mut errors,
    );
    assert!(
        errors
            .iter()
            .any(|error| error.contains("exact recovery source")),
        "{errors:?}"
    );

    let untrusted_job_credential = good.replace(
        "  apply:\n    name:",
        &[
            "  apply:\n    env:\n      GH_TOKEN: ",
            concat!("$", "{{ secrets.GH_PROJECTS_TOKEN }}"),
            "\n    name:",
        ]
        .concat(),
    );
    let mut errors = Vec::new();
    validate_agent_plugin_workflow_text(
        "execution_state_sync.yml",
        &untrusted_job_credential,
        AgentPluginWorkflowKind::ExecutionState,
        &mut errors,
    );
    assert!(
        errors
            .iter()
            .any(|error| error.contains("job must not expose credentials")),
        "{errors:?}"
    );
}

#[test]
fn dependency_candidate_evidence_keeps_upstream_artifact_ids_as_json_integers() {
    let good = include_str!("../../../.github/workflows/dependency-remediation.yml");
    let mut errors = Vec::new();
    validate_agent_plugin_workflow_text(
        "dependency-remediation.yml",
        good,
        AgentPluginWorkflowKind::CodexPlugins,
        &mut errors,
    );
    assert!(errors.is_empty(), "{errors:?}");

    let candidate = include_str!("../dependency_remediation/candidate.rs");
    let mut errors = Vec::new();
    super::runtime_workflows::validate_dependency_candidate_artifact_ids(candidate, &mut errors);
    assert!(errors.is_empty(), "{errors:?}");

    let quoted_id = candidate.replace(
        "Ok(json!({\"id\":id,\"name\":name,\"digest\":digest}))",
        "Ok(json!({\"id\":id.to_string(),\"name\":name,\"digest\":digest}))",
    );
    let mut errors = Vec::new();
    super::runtime_workflows::validate_dependency_candidate_artifact_ids(&quoted_id, &mut errors);
    assert!(
        errors
            .iter()
            .any(|error| error.contains("positive JSON integers")),
        "{errors:?}"
    );
}

#[test]
fn public_release_workflows_do_not_depend_on_consumer_tool_acquisition() {
    let temp = TempDir::new().unwrap();
    let workflow_dir = temp.path().join(".github/workflows");
    fs::create_dir_all(&workflow_dir).unwrap();
    let contracts = [
        (
            "release-publish.yml",
            "workflow_dispatch:\nExplicitly authorize publishing exact current main\ncargo publish -p git-slop --locked --no-verify\ncargo xtask verify-crate\nverified-registry-crate\ngh release create \"$TAG\" --draft --notes-file release-notes.md --title \"$TAG\" --target \"$REVISION\" --verify-tag\nmarketplace-ready:\nonly manual approval for the release\nDispatch immutable release identity to Homebrew tap\nsecrets.HOMEBREW_TAP_DISPATCH_TOKEN\n",
        ),
        (
            "release-published.yml",
            "types: [published]\nrelease-manifest.json\nSummarize publication verification\nwithout any Actions environment approval\nDispatch immutable release identity to Scoop bucket\nsecrets.SCOOP_BUCKET_DISPATCH_TOKEN\n--repo coreycoto/scoop-bucket\n",
        ),
        (
            "homebrew-handoff.yml",
            "workflow_dispatch:\nenvironment: release\nsecrets.HOMEBREW_TAP_DISPATCH_TOKEN\nhttps://static.crates.io/crates/git-slop/\n--repo coreycoto/homebrew-tap\n--ref main\n",
        ),
    ];
    for (name, contract) in contracts {
        fs::write(workflow_dir.join(name), contract).unwrap();
    }
    let mut errors = Vec::new();
    validate_release_workflow(temp.path(), &mut errors);
    assert!(errors.is_empty(), "{errors:?}");

    for (name, contract) in contracts {
        for forbidden in [
            "AGENT_PLUGINS_READ_TOKEN",
            "scripts/with-gh-steward.sh",
            "coreycoto/gh-steward",
            ".agents/gh-steward.lock.json",
        ] {
            fs::write(workflow_dir.join(name), format!("{contract}{forbidden}\n")).unwrap();
            let mut errors = Vec::new();
            validate_release_workflow(temp.path(), &mut errors);
            assert!(
                errors.iter().any(|error| error.contains(forbidden)),
                "{name}: {forbidden}: {errors:?}"
            );
            fs::write(workflow_dir.join(name), contract).unwrap();
        }
    }
}

fn valid_gh_steward_lock() -> JsonValue {
    json!({
        "schema_version": 1,
        "repository": GH_STEWARD_REPOSITORY,
        "version": GH_STEWARD_VERSION,
        "source_revision": "a".repeat(40),
        "asset_sha256": {
            "darwin/amd64": "1".repeat(64),
            "darwin/arm64": "2".repeat(64),
            "linux/amd64": "3".repeat(64),
            "linux/arm64": "4".repeat(64),
        }
    })
}

fn safe_codex_workflow() -> &'static str {
    r##"name: Fixture
on: workflow_dispatch
jobs:
  validate:
    runs-on: ubuntu-latest
    steps:
      - name: Checkout trusted source
        uses: actions/checkout@v6
        with:
          persist-credentials: false
      - name: Acquire gh-steward
        run: scripts/with-gh-steward.sh --prepare
      - name: Verify gh-steward
        run: scripts/with-gh-steward.sh --verify
      - name: Install public plugins
        run: scripts/prepare-codex-plugins.sh "$RUNNER_TEMP/codex-runtime/.codex"
      - name: Run Codex
        uses: openai/codex-action@86365089eb2b84e0a8fb0717b304f8bdcb13b20e
        with:
          codex-home: ${{ runner.temp }}/codex-runtime/.codex
          model: gpt-6-luna
      - name: Annotation
        run: echo "# additional trusted annotation"
"##
}
