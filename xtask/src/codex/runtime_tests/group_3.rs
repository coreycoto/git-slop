#[test]
fn public_release_workflows_reject_private_runtime_surfaces() {
    let temp = TempDir::new().unwrap();
    let workflow_dir = temp.path().join(".github/workflows");
    fs::create_dir_all(&workflow_dir).unwrap();
    let contracts = [
        (
            "release-publish.yml",
            r#"workflow_dispatch:
Explicitly authorize publishing exact current main
cargo publish -p git-slop --locked --no-verify
cargo xtask verify-crate
verified-registry-crate
gh release create "$TAG" --draft --notes-file release-notes.md --title "$TAG" --target "$REVISION" --verify-tag
marketplace-ready:
only manual approval for the release
Dispatch immutable release identity to Homebrew tap
secrets.HOMEBREW_TAP_DISPATCH_TOKEN
"#,
        ),
        (
            "release-published.yml",
            r#"types: [published]
release-manifest.json
Summarize publication verification
without any Actions environment approval
Dispatch immutable release identity to Scoop bucket
secrets.SCOOP_BUCKET_DISPATCH_TOKEN
--repo coreycoto/scoop-bucket
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
        uses: openai/codex-action@52fe01ec70a42f454c9d2ebd47598f9fd6893d56
        env:
          GH_TOKEN: ${{ github.token }}
        with:
          prompt-file: ${{ runner.temp }}/dependency-remediation-trusted/dependency-remediation.md
          output-schema-file: ${{ runner.temp }}/dependency-remediation-trusted/dependency-remediation.json
          codex-home: ${{ runner.temp }}/codex-runtime/.codex
          codex-args: >-
            ["--profile","ci_mutation"]
          allow-bot-users: dependabot[bot]
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
