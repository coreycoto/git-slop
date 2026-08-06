use std::fs;

use serde_json::{Value as JsonValue, json};
use tempfile::TempDir;

use super::runtime_manifest::{
    EXPECTED_CHECKSUMS, EXPECTED_MARKETPLACE_NAME, EXPECTED_PLUGIN_SHA, EXPECTED_RELEASE_MANIFEST,
    EXPECTED_RUNTIME_ARCHIVE, EXPECTED_RUNTIME_MEMBER, EXPECTED_RUNTIME_REPOSITORY,
    EXPECTED_RUNTIME_SHA256, EXPECTED_RUNTIME_SIZE, EXPECTED_RUNTIME_TAG, EXPECTED_RUNTIME_TARGET,
    EXPECTED_RUNTIME_VERSION, INSTALLED_PLUGIN_NAME, validate_agent_plugin_wrapper_text,
    validate_marketplace_source_manifest,
};
use super::runtime_workflows::{AgentPluginWorkflowKind, validate_agent_plugin_workflow_text};
use super::{EXPECTED_PLUGIN_URL, validate_release_workflow};

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

#[test]
fn public_release_workflows_reject_private_runtime_surfaces() {
    let temp = TempDir::new().unwrap();
    let workflow_dir = temp.path().join(".github/workflows");
    fs::create_dir_all(&workflow_dir).unwrap();
    let contracts = [
        (
            "release-publish.yml",
            r#"workflow_dispatch:
cargo publish -p git-slop --locked --no-verify
cargo xtask verify-crate
verified-registry-crate
gh release create "$TAG" --draft --generate-notes --title "$TAG" --verify-tag
marketplace-ready:
Dispatch immutable release identity to Homebrew tap
secrets.HOMEBREW_TAP_DISPATCH_TOKEN
"#,
        ),
        (
            "release-published.yml",
            r#"types: [published]
release-manifest.json
Summarize publication verification
without another environment approval
"#,
        ),
        (
            "homebrew-handoff.yml",
            r#"workflow_dispatch:
environment: release
secrets.HOMEBREW_TAP_DISPATCH_TOKEN
https://static.crates.io/crates/git-slop/
--repo coreycoto/homebrew-tap
--ref main
"#,
        ),
    ];
    for (name, contract) in contracts {
        fs::write(workflow_dir.join(name), contract).unwrap();
    }
    let mut errors = Vec::new();
    validate_release_workflow(temp.path(), &mut errors);
    assert_eq!(errors, Vec::<String>::new());

    for (name, contract) in contracts {
        for private_surface in [
            "AGENT_PLUGINS_READ_TOKEN",
            "scripts/with-agent-plugins.sh",
            "coreycoto/agent-plugins",
        ] {
            fs::write(
                workflow_dir.join(name),
                format!("{contract}{private_surface}\n"),
            )
            .unwrap();
            let mut errors = Vec::new();
            validate_release_workflow(temp.path(), &mut errors);
            assert!(
                errors.iter().any(|error| error.contains(private_surface)),
                "missing {private_surface:?} from {name}: {errors:?}"
            );
            fs::write(workflow_dir.join(name), contract).unwrap();
        }
    }
}

fn valid_marketplace_source_manifest() -> JsonValue {
    json!({
        "marketplace_name": EXPECTED_MARKETPLACE_NAME,
        "source_url": EXPECTED_PLUGIN_URL,
        "ref": EXPECTED_PLUGIN_SHA,
        "required_plugin": INSTALLED_PLUGIN_NAME,
        "runtime_release": {
            "repository": EXPECTED_RUNTIME_REPOSITORY,
            "tag": EXPECTED_RUNTIME_TAG,
            "version": EXPECTED_RUNTIME_VERSION,
            "target": EXPECTED_RUNTIME_TARGET,
            "archive": EXPECTED_RUNTIME_ARCHIVE,
            "member": EXPECTED_RUNTIME_MEMBER,
            "sha256": EXPECTED_RUNTIME_SHA256,
            "size": EXPECTED_RUNTIME_SIZE,
            "release_manifest": EXPECTED_RELEASE_MANIFEST,
            "checksums": EXPECTED_CHECKSUMS,
        }
    })
}

fn valid_wrapper_fixture() -> &'static str {
    r#"#!/usr/bin/env bash
manifest=.agents/plugins/marketplace-source.json
runtime_root="${AGENT_PLUGINS_RUNTIME_ROOT:-${RUNNER_TEMP}/agent-plugins-runtime}"
[[ -z "${RUNNER_TOOL_CACHE-}" ]] || die "runtime root must not use RUNNER_TOOL_CACHE or an Actions cache"
release_manifest=release-manifest.json
checksums=SHA256SUMS
runtime_executable="$runtime_root/agent-plugins"
release_contract='.source_revision == $revision and .sha256 == $sha256 and .size == $size'
dependency_lock_format=pex-lock-from-hashed-requirements
archive_sha_error='runtime archive SHA-256 mismatch'
archive_size_error='runtime archive size mismatch'
revision_error='installed runtime source revision mismatch'
smoke_error='isolated interpreter import and provenance smoke'

download_release_assets() {
  gh release download
}

prepare_runtime() {
  download_release_assets
  sha256sum "$runtime_executable"
  "$runtime_executable" --version
  "$runtime_executable" --source-revision
  unset AGENT_PLUGINS_READ_TOKEN
}

verify_install() {
  sha256sum "$runtime_executable"
  "$runtime_executable" --version
  "$runtime_executable" --source-revision
}

exec_runtime() {
  exec env PEX_IGNORE_RCFILES=1 "$runtime_executable" "$@"
}

exec_python_compatibility() {
  unset AGENT_PLUGINS_READ_TOKEN GH_TOKEN GITHUB_TOKEN
  exec env PEX_INTERPRETER=1 "$runtime_executable" "$@"
}

case "${1:-}" in
  --prepare)
    prepare_runtime
    ;;
  --verify)
    verify_install
    ;;
  python)
    shift
    unset AGENT_PLUGINS_READ_TOKEN
    exec_python_compatibility "$@"
    ;;
  *)
    verify_install
    exec_runtime "$@"
    ;;
esac
"#
}

fn safe_marketplace_workflow() -> &'static str {
    r#"name: Runtime fixture
on: workflow_dispatch
jobs:
  validate:
    runs-on: ubuntu-latest
    steps:
      - name: Checkout trusted source
        uses: actions/checkout@v6
      - name: Detect Codex credentials
        id: codex_preflight
        run: echo "enabled=true" >> "$GITHUB_OUTPUT"
      - name: Prepare artifact roots
        if: steps.codex_preflight.outputs.enabled == 'true'
        run: |
          mkdir -p .artifacts/codex .artifacts/docs-taxonomy
          jq -n '{}' > .artifacts/docs-taxonomy/run-context.json
      - name: Acquire runtime
        env:
          AGENT_PLUGINS_READ_TOKEN: ${{ secrets.AGENT_PLUGINS_READ_TOKEN }}
        run: scripts/with-agent-plugins.sh --prepare
      - name: Verify runtime
        run: scripts/with-agent-plugins.sh --verify
      - name: Install marketplace
        run: scripts/with-agent-plugins.sh marketplace install
"#
}

fn safe_dependency_workflow() -> &'static str {
    r#"name: Dependency fixture
on: pull_request_target
jobs:
  remediate:
    if: github.event_name != 'pull_request_target' || github.actor == 'dependabot[bot]'
    runs-on: ubuntu-latest
    steps:
      - name: Checkout trusted base
        uses: actions/checkout@v6
        with:
          persist-credentials: false
          ref: ${{ github.event_name == 'pull_request_target' && github.event.pull_request.base.sha || github.sha }}
      - name: Set up Rust
        uses: dtolnay/rust-toolchain@trusted
      - name: Validate trusted Codex surface
        run: cargo xtask validate-codex
      - name: Acquire runtime
        env:
          AGENT_PLUGINS_READ_TOKEN: ${{ secrets.AGENT_PLUGINS_READ_TOKEN }}
        run: scripts/with-agent-plugins.sh --prepare
      - name: Verify runtime
        run: scripts/with-agent-plugins.sh --verify
      - name: Snapshot trusted Codex runtime inputs
        run: |
          trusted_root="$RUNNER_TEMP/dependency-remediation-trusted"
          codex_home="$RUNNER_TEMP/codex-runtime/.codex"
          mkdir -p "$trusted_root" "$codex_home/agents"
          cp .codex/config.toml "$codex_home/config.toml"
          cp .codex/*.config.toml "$codex_home/"
          cp -R .codex/agents/. "$codex_home/agents/"
          cp .github/codex/prompts/dependency-remediation.md \
            "$trusted_root/dependency-remediation.md"
          cp .github/codex/schemas/dependency-remediation.json \
            "$trusted_root/dependency-remediation.json"
      - name: Install marketplace
        run: scripts/with-agent-plugins.sh marketplace install
      - name: Checkout requested head
        uses: actions/checkout@v6
        with:
          persist-credentials: false
          ref: ${{ github.event.pull_request.head.sha }}
      - name: Run Codex remediation
        uses: openai/codex-action@v1
        env:
          GH_TOKEN: ${{ github.token }}
        with:
          prompt-file: ${{ runner.temp }}/dependency-remediation-trusted/dependency-remediation.md
          output-schema-file: ${{ runner.temp }}/dependency-remediation-trusted/dependency-remediation.json
          codex-home: ${{ runner.temp }}/codex-runtime/.codex
          codex-args: >-
            ["--profile","ci_mutation"]
"#
}

fn safe_execution_state_workflow() -> &'static str {
    r#"name: Execution-state fixture
on:
  pull_request_target:
jobs:
  sync:
    if: github.event_name != 'pull_request_target' || github.event.pull_request.head.repo.full_name == github.repository
    runs-on: ubuntu-latest
    env:
      ROADMAP_GH_TOKEN_SOURCE: ${{ secrets.GH_PROJECTS_TOKEN != '' && 'GH_PROJECTS_TOKEN' || 'github.token' }}
    steps:
      - name: Checkout trusted base
        uses: actions/checkout@v6
        with:
          persist-credentials: false
          ref: ${{ github.event_name == 'pull_request_target' && github.event.action != 'closed' && github.event.pull_request.base.sha || github.sha }}
      - name: Prepare artifact root
        id: artifact-root
        run: |
          root=".artifacts/execution-state/fixture"
          mkdir -p "$root"
          jq -n '{}' > "$root/run-context.json"
          echo "path=$root" >> "$GITHUB_OUTPUT"
      - name: Acquire runtime
        env:
          AGENT_PLUGINS_READ_TOKEN: ${{ secrets.AGENT_PLUGINS_READ_TOKEN }}
        run: scripts/with-agent-plugins.sh --prepare
      - name: Verify runtime
        run: scripts/with-agent-plugins.sh --verify
      - name: Snapshot project
        env:
          GH_TOKEN: ${{ secrets.GH_PROJECTS_TOKEN != '' && secrets.GH_PROJECTS_TOKEN || github.token }}
        run: scripts/with-agent-plugins.sh github project-snapshot --require-live
      - name: Sync state
        env:
          GH_TOKEN: ${{ secrets.GH_PROJECTS_TOKEN != '' && secrets.GH_PROJECTS_TOKEN || github.token }}
        run: scripts/with-agent-plugins.sh github execution-state --json-path result.json sync
      - name: Upload execution artifacts
        if: ${{ (failure() || github.event_name == 'workflow_dispatch') && steps.artifact-root.outputs.path != '' }}
        uses: actions/upload-artifact@v7
        with:
          path: ${{ steps.artifact-root.outputs.path }}
          include-hidden-files: true
          if-no-files-found: error
          retention-days: 14
"#
}
