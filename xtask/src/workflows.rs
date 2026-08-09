use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use serde_yaml::Value as YamlValue;

const CODEX_WORKFLOWS: [&str; 4] = [
    "dependency-remediation.yml",
    "docs-taxonomy.yml",
    "governance-reconcile.yml",
    "merge-on-green.yml",
];

const AGENT_PLUGIN_WORKFLOWS: [&str; 5] = [
    "dependency-remediation.yml",
    "docs-taxonomy.yml",
    "governance-reconcile.yml",
    "merge-on-green.yml",
    "execution_state_sync.yml",
];

const AGENT_PLUGIN_WRAPPER: &str = "scripts/with-agent-plugins.sh";
const PREPARE_COMMAND: &str = "scripts/with-agent-plugins.sh --prepare";
const VERIFY_COMMAND: &str = "scripts/with-agent-plugins.sh --verify";
const MARKETPLACE_COMMAND: &str = "scripts/with-agent-plugins.sh marketplace install";
const RELEASE_TARGETS: [&str; 7] = [
    "aarch64-apple-darwin",
    "aarch64-pc-windows-msvc",
    "aarch64-unknown-linux-gnu",
    "x86_64-apple-darwin",
    "x86_64-pc-windows-msvc",
    "x86_64-unknown-linux-gnu",
    "x86_64-unknown-linux-musl",
];
const RELEASE_TARGET_RUNNERS: [(&str, &str); 7] = [
    ("macos-15", "aarch64-apple-darwin"),
    ("windows-11-arm", "aarch64-pc-windows-msvc"),
    ("ubuntu-22.04-arm", "aarch64-unknown-linux-gnu"),
    ("macos-15-intel", "x86_64-apple-darwin"),
    ("windows-2025", "x86_64-pc-windows-msvc"),
    ("ubuntu-22.04", "x86_64-unknown-linux-gnu"),
    ("ubuntu-22.04", "x86_64-unknown-linux-musl"),
];
const RELEASE_CHECKOUT_ACTION: &str = "actions/checkout@3d3c42e5aac5ba805825da76410c181273ba90b1";
const PUBLIC_RELEASE_WORKFLOWS: [&str; 3] = [
    "release-publish.yml",
    "release-published.yml",
    "homebrew-handoff.yml",
];
const PRIVATE_RUNTIME_SURFACES: [&str; 8] = [
    "AGENT_PLUGINS_READ_TOKEN",
    "AGENT_PLUGINS_GIT_TOKEN",
    AGENT_PLUGIN_WRAPPER,
    ".agents/plugins/marketplace-source.json",
    "coreycoto/agent-plugins",
    "agent-plugins-marketplace",
    "agent-plugins-runtime",
    "marketplace install",
];
const CRATES_IO_VERSION_ENDPOINT: &str = "https://crates.io/api/v1/crates/git-slop/${VERSION}";
const CRATES_IO_RELEASE_USER_AGENT: &str =
    r#"--user-agent "git-slop-release-workflow/1 (https://github.com/coreycoto/git-slop)""#;
const CRATES_IO_VERSION_REQUESTS: usize = 4;
const CRATES_IO_AUTH_ACTION: &str =
    "rust-lang/crates-io-auth-action@c6f97d42243bad5fab37ca0427f495c86d5b1a18";
const CRATES_IO_AUTH_STEP: &str = "Exchange GitHub OIDC identity for crates.io token";
const CRATES_IO_PUBLISH_STEP: &str = "Publish exact crates.io package";
const CRATES_IO_PUBLISH_CONDITION: &str =
    "needs.candidate.outputs.mode == 'publish' && steps.state.outputs.crate-exists != 'true'";
const CRATES_IO_TEMP_TOKEN: &str = "${{ steps.crates-io-auth.outputs.token }}";
const RELEASE_DISPATCH_AUTHORIZATION: &str = "Explicitly authorize publishing exact current main, or recover an already-published immutable crate.";

pub fn validate(repo_root: &Path) -> Vec<String> {
    let mut errors = Vec::new();
    let workflows = repo_root.join(".github/workflows");

    for name in CODEX_WORKFLOWS {
        let Some(text) = read(&workflows.join(name), &mut errors) else {
            continue;
        };
        if name == "dependency-remediation.yml" {
            for trusted_snapshot in [
                r#"codex_home="$RUNNER_TEMP/codex-runtime/.codex""#,
                r#"cp .codex/config.toml "$codex_home/config.toml""#,
                r#"cp .codex/*.config.toml "$codex_home/""#,
                r#"cp -R .codex/agents/. "$codex_home/agents/""#,
                "cp .github/codex/prompts/dependency-remediation.md",
                "cp .github/codex/schemas/dependency-remediation.json",
                "prompt-file: ${{ runner.temp }}/dependency-remediation-trusted/dependency-remediation.md",
                "output-schema-file: ${{ runner.temp }}/dependency-remediation-trusted/dependency-remediation.json",
            ] {
                require(&text, trusted_snapshot, name, &mut errors);
            }
        } else {
            require(
                &text,
                r#"cp .codex/config.toml "$RUNNER_TEMP/codex-runtime/.codex/config.toml""#,
                name,
                &mut errors,
            );
            require(
                &text,
                r#"cp .codex/*.config.toml "$RUNNER_TEMP/codex-runtime/.codex/""#,
                name,
                &mut errors,
            );
        }
        require(&text, MARKETPLACE_COMMAND, name, &mut errors);
        require(
            &text,
            "codex-home: ${{ runner.temp }}/codex-runtime/.codex",
            name,
            &mut errors,
        );
        require(&text, r#""--profile","ci_mutation""#, name, &mut errors);
        require(&text, "cargo xtask validate-codex", name, &mut errors);
        forbid(&text, "uv sync", name, &mut errors);
        forbid(
            &text,
            "scripts/validate_codex_surface.py",
            name,
            &mut errors,
        );
    }

    validate_agent_plugin_runtime(&workflows, &mut errors);
    validate_public_release_workflows(repo_root, &mut errors);

    for name in ["docs-taxonomy.yml", "merge-on-green.yml"] {
        if let Some(text) = read(&workflows.join(name), &mut errors) {
            forbid(&text, "gpt-5.4-nano", name, &mut errors);
            require(&text, r#""--model","gpt-5.6-luna""#, name, &mut errors);
        }
    }

    validate_action_versions(repo_root, &workflows, &mut errors);
    validate_artifacts(&workflows, &mut errors);
    validate_dogfood(&workflows, &mut errors);
    validate_ci(repo_root, &workflows, &mut errors);

    errors
}

pub(crate) fn validate_public_release_workflows(repo_root: &Path, errors: &mut Vec<String>) {
    let workflows = repo_root.join(".github/workflows");
    let Some((publish_text, publish)) = load_workflow(&workflows, "release-publish.yml", errors)
    else {
        return;
    };
    let Some((relay_text, relay)) = load_workflow(&workflows, "release-published.yml", errors)
    else {
        return;
    };
    let Some((homebrew_text, homebrew)) = load_workflow(&workflows, "homebrew-handoff.yml", errors)
    else {
        return;
    };

    for (name, text) in [
        ("release-publish.yml", publish_text.as_str()),
        ("release-published.yml", relay_text.as_str()),
        ("homebrew-handoff.yml", homebrew_text.as_str()),
    ] {
        validate_no_private_runtime(name, text, errors);
    }
    validate_release_publish(&publish_text, &publish, errors);
    validate_release_relay(&relay_text, &relay, errors);
    validate_homebrew_handoff(&homebrew, errors);
}

fn load_workflow(
    workflows: &Path,
    name: &str,
    errors: &mut Vec<String>,
) -> Option<(String, YamlValue)> {
    let text = read(&workflows.join(name), errors)?;
    match serde_yaml::from_str::<YamlValue>(&text) {
        Ok(payload) => Some((text, payload)),
        Err(error) => {
            errors.push(format!("Unable to parse {name}: {error}"));
            None
        }
    }
}

fn validate_no_private_runtime(name: &str, text: &str, errors: &mut Vec<String>) {
    for forbidden in PRIVATE_RUNTIME_SURFACES {
        forbid(text, forbidden, name, errors);
    }
}

fn validate_release_publish(text: &str, payload: &YamlValue, errors: &mut Vec<String>) {
    let name = "release-publish.yml";
    validate_crates_io_api_client(text, name, errors);
    require_exact_trigger(payload, name, "workflow_dispatch", errors);
    let Some(dispatch) = payload.get("on").and_then(|on| on.get("workflow_dispatch")) else {
        return;
    };
    let version_input = dispatch
        .get("inputs")
        .and_then(|inputs| inputs.get("version"));
    if !version_input.is_some_and(|input| {
        input.get("required").and_then(YamlValue::as_bool) == Some(true)
            && input.get("type").and_then(YamlValue::as_str) == Some("string")
    }) {
        errors.push(format!(
            "{name} workflow_dispatch must require a string version input."
        ));
    }
    let mode_input = dispatch.get("inputs").and_then(|inputs| inputs.get("mode"));
    let mode_options = mode_input
        .and_then(|input| input.get("options"))
        .and_then(YamlValue::as_sequence)
        .map(|options| {
            options
                .iter()
                .filter_map(YamlValue::as_str)
                .collect::<Vec<_>>()
        });
    if !mode_input.is_some_and(|input| {
        input.get("required").and_then(YamlValue::as_bool) == Some(true)
            && input.get("type").and_then(YamlValue::as_str) == Some("choice")
            && input.get("default").and_then(YamlValue::as_str) == Some("publish")
    }) || mode_options != Some(vec!["publish", "recover"])
    {
        errors.push(format!(
            "{name} workflow_dispatch must require an exact publish-or-recover mode choice."
        ));
    }
    if mode_input
        .and_then(|input| input.get("description"))
        .and_then(YamlValue::as_str)
        != Some(RELEASE_DISPATCH_AUTHORIZATION)
    {
        errors.push(format!(
            "{name} workflow_dispatch must describe dispatch as explicit publication authorization."
        ));
    }
    for recovery_input in ["recovery_revision", "recovery_crate_sha256"] {
        let valid = dispatch
            .get("inputs")
            .and_then(|inputs| inputs.get(recovery_input))
            .is_some_and(|input| {
                input.get("required").and_then(YamlValue::as_bool) == Some(false)
                    && input.get("type").and_then(YamlValue::as_str) == Some("string")
            });
        if !valid {
            errors.push(format!(
                "{name} workflow_dispatch must define optional string input {recovery_input}."
            ));
        }
    }

    let Some(jobs) = payload.get("jobs").and_then(YamlValue::as_mapping) else {
        errors.push(format!("{name} must define jobs."));
        return;
    };
    require_exact_job_set(
        jobs,
        name,
        &[
            "candidate",
            "candidate-targets",
            "candidate-distribution",
            "candidate-homebrew-audit",
            "publish-crate",
            "build",
            "draft-release",
            "draft-action-smoke",
            "marketplace-ready",
        ],
        errors,
    );

    let candidate = job(jobs, "candidate", name, errors);
    let candidate_targets = job(jobs, "candidate-targets", name, errors);
    let candidate_distribution = job(jobs, "candidate-distribution", name, errors);
    let candidate_homebrew_audit = job(jobs, "candidate-homebrew-audit", name, errors);
    let publish_crate = job(jobs, "publish-crate", name, errors);
    let build = job(jobs, "build", name, errors);
    let draft = job(jobs, "draft-release", name, errors);
    let draft_action_smoke = job(jobs, "draft-action-smoke", name, errors);
    let marketplace_ready = job(jobs, "marketplace-ready", name, errors);

    if let Some(candidate) = candidate {
        for (output, expected) in [
            (
                "mode",
                "${{ steps.identity.outputs.mode || steps.recovery-identity.outputs.mode }}",
            ),
            (
                "revision",
                "${{ steps.identity.outputs.revision || steps.recovery-identity.outputs.revision }}",
            ),
            (
                "control-revision",
                "${{ steps.identity.outputs.control-revision || steps.recovery-identity.outputs.control-revision }}",
            ),
            (
                "crate-sha256",
                "${{ steps.package.outputs.crate-sha256 || steps.recovery-package.outputs.crate-sha256 }}",
            ),
        ] {
            if candidate
                .get("outputs")
                .and_then(|outputs| outputs.get(output))
                .and_then(YamlValue::as_str)
                != Some(expected)
            {
                errors.push(format!(
                    "{name} candidate output {output} must select the exact publish or recovery identity."
                ));
            }
        }
        let Some(identity_run) =
            step_run(candidate, "Require exact current main and release identity")
        else {
            errors.push(format!(
                "{name} candidate must validate exact current main."
            ));
            return;
        };
        for required in [
            "test \"$GITHUB_REF\" = \"refs/heads/main\"",
            "+refs/heads/main:refs/remotes/origin/main",
            "test \"$revision\" = \"$(git rev-parse refs/remotes/origin/main)\"",
            "echo \"control-revision=$revision\"",
            "test -z \"$(git status --short)\"",
        ] {
            require(identity_run, required, name, errors);
        }
        if named_step(candidate, "Require exact current main and release identity")
            .and_then(|step| step.get("if"))
            .and_then(YamlValue::as_str)
            != Some("inputs.mode == 'publish'")
        {
            errors.push(format!(
                "{name} normal candidate identity must run only in publish mode."
            ));
        }
        let Some(recovery_identity) =
            step_run(candidate, "Require explicit immutable recovery identity")
        else {
            errors.push(format!(
                "{name} candidate must validate the explicit recovery identity."
            ));
            return;
        };
        for required in [
            "[[ \"$REVISION\" =~ ^[0-9a-f]{40}$ ]]",
            "[[ \"$EXPECTED_CRATE_SHA256\" =~ ^[0-9a-f]{64}$ ]]",
            "control_revision=\"$(git rev-parse HEAD)\"",
            "test \"$control_revision\" = \"$(git rev-parse refs/remotes/origin/main)\"",
            "echo \"control-revision=$control_revision\"",
            "git cat-file -e \"${REVISION}^{commit}\"",
            "git merge-base --is-ancestor \"$REVISION\" refs/remotes/origin/main",
            "test \"$(git rev-parse \"refs/tags/v${VERSION}^{commit}\")\" = \"$REVISION\"",
        ] {
            require(recovery_identity, required, name, errors);
        }
        if named_step(candidate, "Require explicit immutable recovery identity")
            .and_then(|step| step.get("if"))
            .and_then(YamlValue::as_str)
            != Some("inputs.mode == 'recover'")
        {
            errors.push(format!(
                "{name} recovery identity must run only in recover mode."
            ));
        }
        let Some(preflight_run) = step_run(candidate, "Run full repository preflight") else {
            errors.push(format!(
                "{name} candidate must run the full repository preflight."
            ));
            return;
        };
        for required in [
            "cargo xtask release-prepare --version \"$VERSION\" --check-only",
            "cargo xtask validate",
            "node --test action/*.test.mjs",
            "cargo publish -p git-slop --dry-run --locked",
        ] {
            require(preflight_run, required, name, errors);
        }
        for (step_name, expected_mode) in [
            ("Run full repository preflight", "inputs.mode == 'publish'"),
            (
                "Package and verify candidate crate",
                "inputs.mode == 'publish'",
            ),
            (
                "Validate current recovery tooling",
                "inputs.mode == 'recover'",
            ),
        ] {
            if named_step(candidate, step_name)
                .and_then(|step| step.get("if"))
                .and_then(YamlValue::as_str)
                != Some(expected_mode)
            {
                errors.push(format!(
                    "{name} step {step_name} must be isolated to its release mode."
                ));
            }
        }
        let Some(recovery_package) =
            step_run(candidate, "Download and verify immutable recovery package")
        else {
            errors.push(format!(
                "{name} candidate must acquire an immutable crates.io recovery package."
            ));
            return;
        };
        for required in [
            ".version.num == $version and .version.yanked == false and .version.checksum == $checksum",
            "https://static.crates.io/crates/git-slop/git-slop-${VERSION}.crate",
            "test \"$digest\" = \"$EXPECTED_CRATE_SHA256\"",
            "cargo xtask verify-crate",
            "--revision \"$REVISION\"",
            "--expected-sha256 \"$EXPECTED_CRATE_SHA256\"",
        ] {
            require(recovery_package, required, name, errors);
        }
        if named_step(candidate, "Download and verify immutable recovery package")
            .and_then(|step| step.get("if"))
            .and_then(YamlValue::as_str)
            != Some("inputs.mode == 'recover'")
        {
            errors.push(format!(
                "{name} registry recovery package must be acquired only in recover mode."
            ));
        }
        let Some(identity_summary) = step_run(candidate, "Record immutable release identity")
        else {
            errors.push(format!(
                "{name} candidate must record its immutable recovery inputs."
            ));
            return;
        };
        for required in [
            "[[ \"$CONTROL_REVISION\" =~ ^[0-9a-f]{40}$ ]]",
            "[[ \"$REVISION\" =~ ^[0-9a-f]{40}$ ]]",
            "[[ \"$CRATE_SHA256\" =~ ^[0-9a-f]{64}$ ]]",
            "Immutable release identity",
            "Workflow control revision",
            "Source revision",
            "Crate SHA-256",
            ">> \"$GITHUB_STEP_SUMMARY\"",
        ] {
            require(identity_summary, required, name, errors);
        }
    }

    if let Some(candidate_targets) = candidate_targets {
        require_needs(
            candidate_targets,
            name,
            "candidate-targets",
            &["candidate"],
            errors,
        );
        validate_target_matrix(candidate_targets, name, "candidate-targets", true, errors);
        for required in [
            "Download exact candidate package",
            "Verify and unpack candidate bytes",
            "build-info --format json",
            ".source_dirty == false",
        ] {
            require(text, required, name, errors);
        }
    }

    if let Some(candidate_distribution) = candidate_distribution {
        require_needs(
            candidate_distribution,
            name,
            "candidate-distribution",
            &["candidate", "candidate-targets"],
            errors,
        );
        let Some(run) = step_run(
            candidate_distribution,
            "Dry-run release manifest and crates-backed Formula",
        ) else {
            errors.push(format!(
                "{name} candidate-distribution must dry-run manifest and Formula generation."
            ));
            return;
        };
        for required in [
            r#"-c user.name="git-slop release validation""#,
            r#"-c user.email="actions@users.noreply.github.com""#,
            "cargo xtask release-manifest",
            "--crate-source candidate/crate-source.json",
            "cargo xtask homebrew-formula",
            "sha256sum git-slop.rb >> SHA256SUMS",
            "wc -l < candidate-dist/SHA256SUMS",
            "= \"9\"",
        ] {
            require(run, required, name, errors);
        }
        forbid(
            run,
            "sha256sum git-slop.rb release-manifest.json",
            name,
            errors,
        );
        let upload = named_step(
            candidate_distribution,
            "Upload candidate Formula for Homebrew audit",
        );
        let upload_valid = upload.is_some_and(|step| {
            step.get("uses").and_then(YamlValue::as_str)
                == Some("actions/upload-artifact@043fb46d1a93c77aae656e7c1c64a875d1fc6a0a")
                && step
                    .get("with")
                    .and_then(|with| with.get("name"))
                    .and_then(YamlValue::as_str)
                    == Some("candidate-homebrew-formula")
                && step
                    .get("with")
                    .and_then(|with| with.get("path"))
                    .and_then(YamlValue::as_str)
                    == Some("candidate-dist/git-slop.rb")
                && step
                    .get("with")
                    .and_then(|with| with.get("if-no-files-found"))
                    .and_then(YamlValue::as_str)
                    == Some("error")
                && step
                    .get("with")
                    .and_then(|with| with.get("retention-days"))
                    .and_then(YamlValue::as_u64)
                    == Some(1)
        });
        if !upload_valid {
            errors.push(format!(
                "{name} candidate-distribution must upload only the generated Formula with the pinned bounded artifact contract."
            ));
        }
    }

    if let Some(candidate_homebrew_audit) = candidate_homebrew_audit {
        require_needs(
            candidate_homebrew_audit,
            name,
            "candidate-homebrew-audit",
            &["candidate-distribution"],
            errors,
        );
        if candidate_homebrew_audit
            .get("runs-on")
            .and_then(YamlValue::as_str)
            != Some("macos-26")
        {
            errors.push(format!(
                "{name} candidate-homebrew-audit must run with native Homebrew on macos-26."
            ));
        }
        let Some(run) = step_run(
            candidate_homebrew_audit,
            "Audit candidate Formula with Homebrew",
        ) else {
            errors.push(format!(
                "{name} candidate-homebrew-audit must run the Homebrew audit gate."
            ));
            return;
        };
        for required in [
            "brew tap-new --no-git",
            "brew audit --strict --formula",
            "brew style --formula",
        ] {
            require(run, required, name, errors);
        }
        let setup_action = named_step(candidate_homebrew_audit, "Set up Homebrew")
            .and_then(|step| step.get("uses"))
            .and_then(YamlValue::as_str);
        if setup_action
            != Some("Homebrew/actions/setup-homebrew@df4b09108a1de9d6f995fe68f302b3f68bd6d2ef")
        {
            errors.push(format!(
                "{name} candidate-homebrew-audit must use the pinned Homebrew setup Action."
            ));
        }
        let download = named_step(candidate_homebrew_audit, "Download candidate Formula");
        let download_valid = download.is_some_and(|step| {
            step.get("uses").and_then(YamlValue::as_str)
                == Some("actions/download-artifact@3e5f45b2cfb9172054b4087a40e8e0b5a5461e7c")
                && step
                    .get("with")
                    .and_then(|with| with.get("name"))
                    .and_then(YamlValue::as_str)
                    == Some("candidate-homebrew-formula")
                && step
                    .get("with")
                    .and_then(|with| with.get("path"))
                    .and_then(YamlValue::as_str)
                    == Some("${{ runner.temp }}/candidate-homebrew")
        });
        if !download_valid {
            errors.push(format!(
                "{name} candidate-homebrew-audit must download only the generated Formula with the pinned artifact contract."
            ));
        }
        let audit_formula_path = named_step(
            candidate_homebrew_audit,
            "Audit candidate Formula with Homebrew",
        )
        .and_then(|step| step_env(step, "FORMULA_PATH"));
        if audit_formula_path != Some("${{ runner.temp }}/candidate-homebrew/git-slop.rb") {
            errors.push(format!(
                "{name} candidate-homebrew-audit must audit the exact downloaded Formula path."
            ));
        }
    }

    if let Some(publish_crate) = publish_crate {
        if publish_crate.get("name").and_then(YamlValue::as_str)
            != Some("Dispatch-authorized crates.io publication and exact tag")
        {
            errors.push(format!(
                "{name} publish-crate must identify the dispatch-authorized publication boundary."
            ));
        }
        require_needs(
            publish_crate,
            name,
            "publish-crate",
            &[
                "candidate",
                "candidate-distribution",
                "candidate-homebrew-audit",
            ],
            errors,
        );
        require_environment(publish_crate, name, "publish-crate", "release", errors);
        for (output, expected) in [
            ("mode", "${{ needs.candidate.outputs.mode }}"),
            (
                "control-revision",
                "${{ needs.candidate.outputs.control-revision }}",
            ),
        ] {
            if publish_crate
                .get("outputs")
                .and_then(|outputs| outputs.get(output))
                .and_then(YamlValue::as_str)
                != Some(expected)
            {
                errors.push(format!(
                    "{name} publish-crate output {output} must preserve the trusted workflow control identity."
                ));
            }
        }
        validate_trusted_publishing(text, payload, publish_crate, errors);
        validate_release_homebrew_token_scope(payload, publish_crate, errors);
        validate_release_tag_secret_scope(payload, publish_crate, errors);
        validate_publish_order_and_registry(publish_crate, errors);
        let Some(revalidate) = step_run(
            publish_crate,
            "Revalidate dispatch-authorized release identity",
        ) else {
            errors.push(format!(
                "{name} publish-crate must revalidate the dispatch-authorized release identity."
            ));
            return;
        };
        if named_step(
            publish_crate,
            "Revalidate dispatch-authorized release identity",
        )
        .and_then(|step| step_env(step, "CONTROL_REVISION"))
            != Some("${{ needs.candidate.outputs.control-revision }}")
        {
            errors.push(format!(
                "{name} dispatch-authorized release revalidation must bind the exact trusted workflow control revision."
            ));
        }
        for required in [
            "+refs/heads/main:refs/remotes/origin/main",
            "[[ \"$CONTROL_REVISION\" =~ ^[0-9a-f]{40}$ ]]",
            "test \"$CONTROL_REVISION\" = \"$GITHUB_SHA\"",
            "test \"$CONTROL_REVISION\" = \"$(git rev-parse refs/remotes/origin/main)\"",
            "test \"$REVISION\" = \"$(git rev-parse HEAD)\"",
            "test \"$REVISION\" = \"$CONTROL_REVISION\"",
            "git merge-base --is-ancestor \"$REVISION\" refs/remotes/origin/main",
            "cmp ",
            "crate-source.json\" \"${CANDIDATE_DIR}/reverified-crate-source.json",
            "cargo package -p git-slop --locked --no-verify",
            "target/package/git-slop-${VERSION}.crate",
            ".version.num == $version and .version.yanked == false and .version.checksum == $checksum",
            "test \"$(sha256sum registry-recovery.crate | awk '{print $1}')\" = \"$EXPECTED_CRATE_SHA256\"",
            "cmp registry-recovery.crate \"${CANDIDATE_DIR}/git-slop-${VERSION}.crate\"",
        ] {
            require(revalidate, required, name, errors);
        }
        let Some(summary) = step_run(publish_crate, "Summarize dispatch-authorized publication")
        else {
            errors.push(format!(
                "{name} publish-crate must summarize the dispatch-authorized publication."
            ));
            return;
        };
        for required in [
            "explicit Release Publish workflow dispatch",
            "branch-restricted",
            "adds no reviewer gate",
        ] {
            require(summary, required, name, errors);
        }
    }

    if let Some(build) = build {
        require_needs(build, name, "build", &["publish-crate"], errors);
        validate_target_matrix(build, name, "build", false, errors);
        let rendered_steps = steps(build)
            .into_iter()
            .filter_map(|step| step.get("uses").and_then(YamlValue::as_str))
            .collect::<Vec<_>>();
        if rendered_steps
            .iter()
            .any(|uses| uses.starts_with("actions/checkout@"))
        {
            errors.push(format!(
                "{name} build must compile only the verified registry crate, without a source checkout."
            ));
        }
        for required in [
            "name: verified-registry-crate",
            "registry-source/git-slop-${VERSION}",
            "cargo build --manifest-path \"$package/Cargo.toml\"",
            "build-info --format json",
        ] {
            require(text, required, name, errors);
        }
    }

    if let Some(draft) = draft {
        require_needs(
            draft,
            name,
            "draft-release",
            &["publish-crate", "build"],
            errors,
        );
        if draft
            .get("outputs")
            .and_then(|outputs| outputs.get("release-id"))
            .and_then(YamlValue::as_str)
            != Some("${{ steps.release.outputs.release-id || steps.draft.outputs.release-id }}")
        {
            errors.push(format!(
                "{name} draft-release must expose the exact resolved numeric release ID."
            ));
        }
        if draft
            .get("outputs")
            .and_then(|outputs| outputs.get("release-manifest-sha256"))
            .and_then(YamlValue::as_str)
            != Some("${{ steps.release-install.outputs.release-manifest-sha256 }}")
        {
            errors.push(format!(
                "{name} draft-release must expose the verified release manifest digest."
            ));
        }
        if draft
            .get("outputs")
            .and_then(|outputs| outputs.get("asset-sha256-by-target"))
            .and_then(YamlValue::as_str)
            != Some("${{ steps.release-identity.outputs.asset-sha256-by-target }}")
        {
            errors.push(format!(
                "{name} draft-release must expose verified archive digests by target."
            ));
        }
        if let Some(control_checkout) =
            named_step(draft, "Checkout current recovery control tooling")
        {
            if control_checkout.get("if").and_then(YamlValue::as_str)
                != Some("needs.publish-crate.outputs.mode == 'recover'")
                || control_checkout
                    .get("with")
                    .and_then(|with| with.get("ref"))
                    .and_then(YamlValue::as_str)
                    != Some("${{ needs.publish-crate.outputs.control-revision }}")
                || control_checkout
                    .get("with")
                    .and_then(|with| with.get("path"))
                    .and_then(YamlValue::as_str)
                    != Some("release-control")
                || control_checkout
                    .get("with")
                    .and_then(|with| with.get("sparse-checkout"))
                    .and_then(YamlValue::as_str)
                    != Some("action")
                || control_checkout
                    .get("with")
                    .and_then(|with| with.get("persist-credentials"))
                    .and_then(YamlValue::as_bool)
                    != Some(false)
            {
                errors.push(format!(
                    "{name} recovery control tooling must come from the trusted current-main revision without persisted credentials."
                ));
            }
        } else {
            errors.push(format!(
                "{name} draft-release must checkout current recovery control tooling."
            ));
        }
        let Some(generate) = step_run(draft, "Generate release manifest, checksums, and Formula")
        else {
            errors.push(format!(
                "{name} draft-release must generate final metadata."
            ));
            return;
        };
        require(generate, "cargo xtask sbom --output-dir dist", name, errors);
        require(
            generate,
            "sha256sum git-slop.rb git-slop.cdx.json git-slop.spdx.json >> SHA256SUMS",
            name,
            errors,
        );
        require(
            generate,
            "test \"$(wc -l < dist/SHA256SUMS | tr -d ' ')\" = \"11\"",
            name,
            errors,
        );
        forbid(
            generate,
            "sha256sum git-slop.rb release-manifest.json",
            name,
            errors,
        );
        for required in [
            "gh release create \"$TAG\" --draft --generate-notes --title \"$TAG\" --verify-tag",
            "GIT_SLOP_ALLOW_DRAFT_RELEASE: \"true\"",
            "GIT_SLOP_RELEASE_ID:",
        ] {
            require(text, required, name, errors);
        }
        for forbidden in [
            "gh release edit",
            "--draft=false",
            "-f draft=false",
            "-F draft=false",
            "gh release upload",
            "gh release download",
        ] {
            forbid(text, forbidden, name, errors);
        }
        if let Some(inspect) = step_run(draft, "Inspect existing GitHub Release") {
            for required in [
                "gh api --paginate --slurp \"repos/${GITHUB_REPOSITORY}/releases?per_page=100\"",
                "'[.[][] | select(.tag_name == $tag)]'",
                "match_count=\"$(jq -r 'length' release-matches.json)\"",
                "case \"$match_count\" in",
                "release_id=\"$(jq -er '.id | select(type == \"number\" and . > 0)' release.json)\"",
                "Multiple GitHub Releases use exact tag ${TAG}; refusing ambiguous release mutation.",
                "exit 1",
                "echo \"release-id=$release_id\" >> \"$GITHUB_OUTPUT\"",
            ] {
                require(inspect, required, name, errors);
            }
            forbid(inspect, "releases/tags/${TAG}", name, errors);
            forbid(inspect, "gh release view", name, errors);
        } else {
            errors.push(format!(
                "{name} draft-release must enumerate the exact tag and reject duplicate GitHub Releases."
            ));
        }
        if let Some(refresh) = named_step(draft, "Create or refresh verified draft release") {
            if refresh.get("id").and_then(YamlValue::as_str) != Some("draft") {
                errors.push(format!(
                    "{name} draft refresh must use the stable draft step id."
                ));
            }
            if let Some(run) = refresh.get("run").and_then(YamlValue::as_str) {
                for required in [
                    "gh release create \"$TAG\" --draft --generate-notes --title \"$TAG\" --verify-tag",
                    "for attempt in $(seq 1 10); do",
                    "gh api --paginate --slurp \"repos/${GITHUB_REPOSITORY}/releases?per_page=100\"",
                    "'[.[][] | select(.tag_name == $tag)]'",
                    "match_count=\"$(jq -r 'length' release-matches.json)\"",
                    "if test \"$match_count\" -gt 1; then",
                    "release_id=\"$(jq -er '.[0].id | select(type == \"number\" and . > 0)' release-matches.json)\"",
                    "Multiple GitHub Releases use exact tag ${TAG}; refusing ambiguous release mutation.",
                    "sleep 2",
                    "repos/${GITHUB_REPOSITORY}/releases/${release_id}",
                    ".id == $release_id",
                    "repos/${GITHUB_REPOSITORY}/releases/assets/${asset_id}",
                    "curl --fail-with-body --silent --show-error --connect-timeout 15 --max-time 300",
                    "--request POST",
                    "Accept: application/vnd.github+json",
                    "Authorization: Bearer ${GH_TOKEN}",
                    "X-GitHub-Api-Version: 2022-11-28",
                    "Content-Type: application/octet-stream",
                    "--data-binary \"@${asset}\"",
                    "https://uploads.github.com/repos/${GITHUB_REPOSITORY}/releases/${release_id}/assets?name=${asset_name}",
                    "echo \"release-id=$release_id\" >> \"$GITHUB_OUTPUT\"",
                ] {
                    require(run, required, name, errors);
                }
                forbid(run, "releases/tags/${TAG}", name, errors);
                forbid(run, "gh release upload", name, errors);
                forbid(run, "gh release delete-asset", name, errors);
                forbid(run, "--hostname uploads.github.com", name, errors);
            }
        } else {
            errors.push(format!(
                "{name} must create or refresh the verified draft by numeric release ID."
            ));
        }
        if let Some(verify) = step_run(draft, "Verify published no-op or refreshed draft assets") {
            validate_exact_release_assets(verify, name, "${TAG}", errors);
            for required in [
                "gh api --paginate --slurp \"repos/${GITHUB_REPOSITORY}/releases?per_page=100\"",
                "'[.[][] | select(.tag_name == $tag)]'",
                "for attempt in $(seq 1 10); do",
                "match_count=\"$(jq -r 'length' release-matches.json)\"",
                "if test \"$match_count\" -gt 1; then",
                "Multiple GitHub Releases use exact tag ${TAG}; refusing ambiguous release verification.",
                "sleep 2",
                "test \"$(jq -r 'length' release-matches.json)\" = 1",
                "test \"$(jq -r '.[0].id' release-matches.json)\" = \"$RELEASE_ID\"",
                "repos/${GITHUB_REPOSITORY}/releases/assets/${asset_id}",
                ".id == $release_id",
            ] {
                require(verify, required, name, errors);
            }
            forbid(verify, "releases/tags/${TAG}", name, errors);
            forbid(verify, "gh release download", name, errors);
            if named_step(draft, "Verify published no-op or refreshed draft assets")
                .and_then(|step| step_env(step, "RELEASE_ID"))
                != Some("${{ steps.release.outputs.release-id || steps.draft.outputs.release-id }}")
            {
                errors.push(format!(
                    "{name} final asset verification must bind the exact resolved release ID."
                ));
            }
        } else {
            errors.push(format!(
                "{name} must verify the exact twelve release assets."
            ));
        }
        if let Some(verify_action) =
            named_step(draft, "Verify Action installer against release assets")
        {
            if verify_action.get("id").and_then(YamlValue::as_str) != Some("release-install") {
                errors.push(format!(
                    "{name} draft installer verification must use the stable release-install step id."
                ));
            }
            match verify_action.get("run").and_then(YamlValue::as_str) {
                Some(run) => require(run, "node \"$ACTION_INSTALLER\"", name, errors),
                None => errors.push(format!(
                    "{name} draft installer verification must run the trusted Action installer."
                )),
            }
            for (key, expected) in [
                (
                    "GIT_SLOP_RELEASE_ID",
                    "${{ steps.release.outputs.release-id || steps.draft.outputs.release-id }}",
                ),
                (
                    "ACTION_INSTALLER",
                    "${{ needs.publish-crate.outputs.mode == 'recover' && 'release-control/action/install.mjs' || 'action/install.mjs' }}",
                ),
            ] {
                if step_env(verify_action, key) != Some(expected) {
                    errors.push(format!(
                        "{name} draft installer verification must bind {key} to the exact release or control identity."
                    ));
                }
            }
        } else {
            errors.push(format!(
                "{name} draft-release must verify the Action installer against release assets."
            ));
        }
        if let Some(assert_outputs) = named_step(draft, "Assert exact Action installer outputs") {
            if assert_outputs.get("id").and_then(YamlValue::as_str) != Some("release-identity") {
                errors.push(format!(
                    "{name} draft installer assertion must use the stable release-identity step id."
                ));
            }
            for (key, expected) in [
                (
                    "ACTUAL_VERSION",
                    "${{ steps.release-install.outputs.version }}",
                ),
                (
                    "ACTUAL_REVISION",
                    "${{ steps.release-install.outputs.source-revision }}",
                ),
                (
                    "ACTUAL_TARGET",
                    "${{ steps.release-install.outputs.target }}",
                ),
                (
                    "ACTUAL_CRATE_SHA256",
                    "${{ steps.release-install.outputs.crate-sha256 }}",
                ),
                (
                    "ACTUAL_MANIFEST_SHA256",
                    "${{ steps.release-install.outputs.release-manifest-sha256 }}",
                ),
                (
                    "EXPECTED_CRATE_SHA256",
                    "${{ needs.publish-crate.outputs.crate-sha256 }}",
                ),
            ] {
                if step_env(assert_outputs, key) != Some(expected) {
                    errors.push(format!(
                        "{name} draft installer assertion must bind {key} to the exact named Action output or published crate digest."
                    ));
                }
            }
            match assert_outputs.get("run").and_then(YamlValue::as_str) {
                Some(run) => {
                    for required in [
                        "sha256sum release-verification/release-manifest.json",
                        r#"test "$ACTUAL_VERSION" = "$VERSION""#,
                        r#"test "$ACTUAL_REVISION" = "$REVISION""#,
                        r#"test "$ACTUAL_TARGET" = "x86_64-unknown-linux-gnu""#,
                        r#"test "$ACTUAL_CRATE_SHA256" = "$EXPECTED_CRATE_SHA256""#,
                        r#"test "$ACTUAL_MANIFEST_SHA256" = "$expected_manifest_sha256""#,
                        "reduce .artifacts[] as $artifact ({}; .[$artifact.target] = $artifact.sha256)",
                        r#"length == 7 and all(.[]; test("^[0-9a-f]{64}$"))"#,
                        r#"echo "asset-sha256-by-target=$asset_sha256_by_target" >> "$GITHUB_OUTPUT""#,
                    ] {
                        require(run, required, name, errors);
                    }
                }
                None => errors.push(format!(
                    "{name} draft-release must assert exact named Action installer outputs."
                )),
            }
        } else {
            errors.push(format!(
                "{name} draft-release must assert exact named Action installer outputs."
            ));
        }
    }

    if let Some(draft_action_smoke) = draft_action_smoke {
        if draft_action_smoke.get("if").is_some()
            || draft_action_smoke
                .get("continue-on-error")
                .is_some_and(|value| value.as_bool() != Some(false))
        {
            errors.push(format!(
                "{name} draft-action-smoke must not be conditional or fail open."
            ));
        }
        require_needs(
            draft_action_smoke,
            name,
            "draft-action-smoke",
            &["publish-crate", "draft-release"],
            errors,
        );
        require_exact_job_permission(
            draft_action_smoke,
            name,
            "draft-action-smoke",
            "contents",
            "write",
            errors,
        );
        validate_target_matrix(
            draft_action_smoke,
            name,
            "draft-action-smoke",
            false,
            errors,
        );
        for step_name in [
            "Checkout exact release Action revision",
            "Verify exact immutable Action tag",
            "Run exact release composite Action",
            "Assert installed Action identity",
        ] {
            if let Some(step) = named_step(draft_action_smoke, step_name)
                && (step.get("if").is_some()
                    || step
                        .get("continue-on-error")
                        .is_some_and(|value| value.as_bool() != Some(false)))
            {
                errors.push(format!(
                    "{name} {step_name} must execute unconditionally and fail closed."
                ));
            }
        }
        for (key, expected) in [
            ("VERSION", "${{ needs.publish-crate.outputs.version }}"),
            ("TAG", "${{ needs.publish-crate.outputs.tag }}"),
            ("REVISION", "${{ needs.publish-crate.outputs.revision }}"),
        ] {
            if draft_action_smoke
                .get("env")
                .and_then(|env| env.get(key))
                .and_then(YamlValue::as_str)
                != Some(expected)
            {
                errors.push(format!(
                    "{name} draft Action smoke must bind job environment {key} to {expected}."
                ));
            }
        }
        if let Some(release_checkout) =
            named_step(draft_action_smoke, "Checkout exact release Action revision")
        {
            if release_checkout.get("uses").and_then(YamlValue::as_str)
                != Some(RELEASE_CHECKOUT_ACTION)
                || release_checkout
                    .get("with")
                    .and_then(|with| with.get("ref"))
                    .and_then(YamlValue::as_str)
                    != Some("${{ needs.publish-crate.outputs.revision }}")
                || release_checkout
                    .get("with")
                    .and_then(|with| with.get("fetch-depth"))
                    .and_then(YamlValue::as_i64)
                    != Some(0)
                || release_checkout
                    .get("with")
                    .and_then(|with| with.get("persist-credentials"))
                    .and_then(YamlValue::as_bool)
                    != Some(false)
            {
                errors.push(format!(
                    "{name} draft Action smoke must checkout the exact release revision with tag history and without persisted credentials."
                ));
            }
        } else {
            errors.push(format!(
                "{name} draft Action smoke must checkout the exact release Action revision."
            ));
        }
        if let Some(verify_tag) = step_run(draft_action_smoke, "Verify exact immutable Action tag")
        {
            for required in [
                "test \"$(git rev-parse HEAD)\" = \"$REVISION\"",
                "git fetch --no-tags origin \"refs/tags/${TAG}:refs/tags/${TAG}\"",
                "test \"$(git rev-parse \"refs/tags/${TAG}^{commit}\")\" = \"$REVISION\"",
                "test -z \"$(git status --short)\"",
            ] {
                require(verify_tag, required, name, errors);
            }
        } else {
            errors.push(format!(
                "{name} draft Action smoke must verify the exact immutable Action tag."
            ));
        }
        let action = named_step(draft_action_smoke, "Run exact release composite Action");
        if action
            .and_then(|step| step.get("id"))
            .and_then(YamlValue::as_str)
            != Some("git-slop")
            || action
                .and_then(|step| step.get("uses"))
                .and_then(YamlValue::as_str)
                != Some("./")
        {
            errors.push(format!(
                "{name} draft Action smoke must run the exact checked-out composite Action with the stable git-slop step id."
            ));
        }
        if let Some(action) = action {
            for (key, expected) in [
                ("GIT_SLOP_ALLOW_DRAFT_RELEASE", "true"),
                (
                    "GIT_SLOP_RELEASE_ID",
                    "${{ needs.draft-release.outputs.release-id }}",
                ),
            ] {
                if step_env(action, key) != Some(expected) {
                    errors.push(format!(
                        "{name} draft Action smoke must bind {key} to the exact draft release contract."
                    ));
                }
            }
            for (key, expected) in [
                ("version", "${{ needs.publish-crate.outputs.version }}"),
                ("release-repository", "${{ github.repository }}"),
                ("github-token", "${{ github.token }}"),
                ("policy", "advisory"),
                ("annotations", "false"),
                ("upload-artifact", "false"),
            ] {
                if action
                    .get("with")
                    .and_then(|with| with.get(key))
                    .and_then(YamlValue::as_str)
                    != Some(expected)
                {
                    errors.push(format!(
                        "{name} draft Action smoke must bind composite Action input {key} to {expected}."
                    ));
                }
            }
        }
        if workflow_or_job_env_contains(payload, "GIT_SLOP_GITHUB_TOKEN") {
            errors.push(format!(
                "{name} must not expose GIT_SLOP_GITHUB_TOKEN at workflow or job scope."
            ));
        }
        if yaml_string_occurrences(draft_action_smoke, "${{ github.token }}") != 1
            || steps(draft_action_smoke)
                .into_iter()
                .filter(|step| env_has_key(step, "GIT_SLOP_GITHUB_TOKEN"))
                .count()
                != 0
        {
            errors.push(format!(
                "{name} draft Action smoke must pass github.token only through the composite Action github-token input."
            ));
        }
        if let Some(assert_outputs) =
            named_step(draft_action_smoke, "Assert installed Action identity")
        {
            for (key, expected) in [
                ("ACTUAL_VERSION", "${{ steps.git-slop.outputs.version }}"),
                (
                    "ACTUAL_REVISION",
                    "${{ steps.git-slop.outputs.source-revision }}",
                ),
                ("ACTUAL_TARGET", "${{ steps.git-slop.outputs.target }}"),
                (
                    "ACTUAL_CRATE_SHA256",
                    "${{ steps.git-slop.outputs.crate-sha256 }}",
                ),
                (
                    "ACTUAL_MANIFEST_SHA256",
                    "${{ steps.git-slop.outputs.release-manifest-sha256 }}",
                ),
                (
                    "ACTUAL_ASSET_SHA256",
                    "${{ steps.git-slop.outputs.asset-sha256 }}",
                ),
                (
                    "ANALYSIS_EXIT_CODE",
                    "${{ steps.git-slop.outputs.analysis-exit-code }}",
                ),
                (
                    "POLICY_EXIT_CODE",
                    "${{ steps.git-slop.outputs.policy-exit-code }}",
                ),
                ("STATUS", "${{ steps.git-slop.outputs.status }}"),
                ("EXPECTED_TARGET", "${{ matrix.target }}"),
                (
                    "EXPECTED_CRATE_SHA256",
                    "${{ needs.publish-crate.outputs.crate-sha256 }}",
                ),
                (
                    "EXPECTED_MANIFEST_SHA256",
                    "${{ needs.draft-release.outputs.release-manifest-sha256 }}",
                ),
                (
                    "EXPECTED_ASSET_SHA256",
                    "${{ fromJSON(needs.draft-release.outputs.asset-sha256-by-target)[matrix.target] }}",
                ),
            ] {
                if step_env(assert_outputs, key) != Some(expected) {
                    errors.push(format!(
                        "{name} draft Action smoke must bind {key} to the exact composite Action output or release digest."
                    ));
                }
            }
            if let Some(run) = assert_outputs.get("run").and_then(YamlValue::as_str) {
                for required in [
                    r#"test "$ACTUAL_VERSION" = "$VERSION""#,
                    r#"test "$ACTUAL_REVISION" = "$REVISION""#,
                    r#"test "$ACTUAL_TARGET" = "$EXPECTED_TARGET""#,
                    r#"test "$ACTUAL_CRATE_SHA256" = "$EXPECTED_CRATE_SHA256""#,
                    r#"test "$ACTUAL_MANIFEST_SHA256" = "$EXPECTED_MANIFEST_SHA256""#,
                    r#"test "$ACTUAL_ASSET_SHA256" = "$EXPECTED_ASSET_SHA256""#,
                    r#"test "$ANALYSIS_EXIT_CODE" = 0"#,
                    r#"test "$POLICY_EXIT_CODE" = 0"#,
                    r#"test "$STATUS" = advisory"#,
                ] {
                    require(run, required, name, errors);
                }
            }
        } else {
            errors.push(format!(
                "{name} draft Action smoke must assert the installed composite Action identity and result."
            ));
        }
    }

    if let Some(marketplace_ready) = marketplace_ready {
        if marketplace_ready.get("if").is_some()
            || marketplace_ready
                .get("continue-on-error")
                .is_some_and(|value| value.as_bool() != Some(false))
        {
            errors.push(format!(
                "{name} marketplace-ready must depend normally on successful smoke and fail closed."
            ));
        }
        require_needs(
            marketplace_ready,
            name,
            "marketplace-ready",
            &["publish-crate", "draft-release", "draft-action-smoke"],
            errors,
        );
        let Some(summary) = step_run(marketplace_ready, "Publish Marketplace handoff summary")
        else {
            errors.push(format!(
                "{name} must stop at a Marketplace handoff summary."
            ));
            return;
        };
        if let Some(step) = named_step(marketplace_ready, "Publish Marketplace handoff summary")
            && (step.get("if").is_some()
                || step
                    .get("continue-on-error")
                    .is_some_and(|value| value.as_bool() != Some(false)))
        {
            errors.push(format!(
                "{name} Marketplace handoff summary must execute unconditionally and fail closed."
            ));
        }
        for required in [
            "Marketplace-ready",
            "Open the draft release",
            "only manual approval for the release",
            "already-dispatched Homebrew receiver",
            "existing published release was reverified without mutation, and the dispatch-authorized publication job redispatched",
        ] {
            require(summary, required, name, errors);
        }
    }
}

fn validate_crates_io_api_client(text: &str, name: &str, errors: &mut Vec<String>) {
    let requests = text
        .lines()
        .filter(|line| line.contains(CRATES_IO_VERSION_ENDPOINT))
        .collect::<Vec<_>>();
    if requests.len() != CRATES_IO_VERSION_REQUESTS {
        errors.push(format!(
            "{name} must keep exactly {CRATES_IO_VERSION_REQUESTS} bounded crates.io API version requests."
        ));
    }
    if requests
        .iter()
        .any(|line| !line.contains(CRATES_IO_RELEASE_USER_AGENT))
    {
        errors.push(format!(
            "{name} must identify every crates.io API request with the git-slop release workflow User-Agent and repository contact."
        ));
    }
}

fn validate_trusted_publishing(
    text: &str,
    payload: &YamlValue,
    publish_crate: &YamlValue,
    errors: &mut Vec<String>,
) {
    let name = "release-publish.yml";
    if text
        .lines()
        .any(|line| line.contains("secrets") && line.contains("CARGO_REGISTRY_TOKEN"))
    {
        errors.push(format!(
            "{name} must not reference a long-lived CARGO_REGISTRY_TOKEN secret."
        ));
    }
    if workflow_or_job_env_contains(payload, "CARGO_REGISTRY_TOKEN") {
        errors.push(format!(
            "{name} must not expose CARGO_REGISTRY_TOKEN at workflow or job scope."
        ));
    }

    if payload
        .get("permissions")
        .and_then(|permissions| permissions.get("id-token"))
        .is_some()
    {
        errors.push(format!(
            "{name} must not grant id-token permission at workflow scope."
        ));
    }
    if let Some(jobs) = payload.get("jobs").and_then(YamlValue::as_mapping) {
        for (job_name, job) in jobs {
            let Some(job_name) = job_name.as_str() else {
                continue;
            };
            if !matches!(job_name, "publish-crate" | "draft-release")
                && job
                    .get("permissions")
                    .and_then(|permissions| permissions.get("id-token"))
                    .is_some()
            {
                errors.push(format!(
                    "{name} must not grant id-token permission to {job_name}."
                ));
            }
        }
    }
    let publish_permissions = publish_crate
        .get("permissions")
        .and_then(YamlValue::as_mapping);
    let exact_publish_permissions = publish_permissions.is_some_and(|permissions| {
        permissions.len() == 2
            && permissions.get("contents").and_then(YamlValue::as_str) == Some("write")
            && permissions.get("id-token").and_then(YamlValue::as_str) == Some("write")
    });
    if !exact_publish_permissions {
        errors.push(format!(
            "{name} publish-crate must grant exactly contents: write and id-token: write."
        ));
    }

    let auth_action_steps = workflow_steps(payload)
        .into_iter()
        .filter(|step| {
            step.get("uses")
                .and_then(YamlValue::as_str)
                .is_some_and(|uses| uses.starts_with("rust-lang/crates-io-auth-action@"))
        })
        .collect::<Vec<_>>();
    if auth_action_steps.len() != 1 {
        errors.push(format!(
            "{name} must invoke the crates.io Trusted Publishing action exactly once."
        ));
    }

    let auth_step = named_step(publish_crate, CRATES_IO_AUTH_STEP);
    let auth_step_valid = auth_step.is_some_and(|step| {
        step.get("uses").and_then(YamlValue::as_str) == Some(CRATES_IO_AUTH_ACTION)
            && step.get("id").and_then(YamlValue::as_str) == Some("crates-io-auth")
            && step.get("if").and_then(YamlValue::as_str) == Some(CRATES_IO_PUBLISH_CONDITION)
            && step.as_mapping().is_some_and(|step| {
                mapping_keys(step)
                    == BTreeSet::from([
                        "id".to_owned(),
                        "if".to_owned(),
                        "name".to_owned(),
                        "uses".to_owned(),
                    ])
            })
    });
    if !auth_step_valid {
        errors.push(format!(
            "{name} {CRATES_IO_AUTH_STEP} must use the reviewed SHA-pinned action, stable step id, exact publish-only condition that is unreachable in recovery mode, and no inputs or fail-open behavior."
        ));
    }

    let step_names = steps(publish_crate)
        .into_iter()
        .map(|step| {
            step.get("name")
                .and_then(YamlValue::as_str)
                .unwrap_or_default()
        })
        .collect::<Vec<_>>();
    let state = step_names
        .iter()
        .position(|step| *step == "Inspect immutable registry and tag state");
    let auth = step_names
        .iter()
        .position(|step| *step == CRATES_IO_AUTH_STEP);
    let publish = step_names
        .iter()
        .position(|step| *step == CRATES_IO_PUBLISH_STEP);
    if !matches!((state, auth, publish), (Some(state), Some(auth), Some(publish)) if auth == state + 1 && publish == auth + 1)
    {
        errors.push(format!(
            "{name} must authenticate immediately after immutable registry inspection and publish immediately afterward."
        ));
    }

    let token_steps = workflow_steps(payload)
        .into_iter()
        .filter(|step| env_has_key(step, "CARGO_REGISTRY_TOKEN"))
        .collect::<Vec<_>>();
    if token_steps.len() != 1 {
        errors.push(format!(
            "{name} must bind the temporary CARGO_REGISTRY_TOKEN to exactly one step."
        ));
        return;
    }
    let step = token_steps[0];
    if step.get("name").and_then(YamlValue::as_str) != Some(CRATES_IO_PUBLISH_STEP) {
        errors.push(format!(
            "{name} must expose the temporary CARGO_REGISTRY_TOKEN only to {CRATES_IO_PUBLISH_STEP}."
        ));
    }
    let publish_env = step.get("env").and_then(YamlValue::as_mapping);
    if !publish_env.is_some_and(|env| {
        env.len() == 1
            && env.get("CARGO_REGISTRY_TOKEN").and_then(YamlValue::as_str)
                == Some(CRATES_IO_TEMP_TOKEN)
    }) || yaml_string_occurrences(payload, CRATES_IO_TEMP_TOKEN) != 1
        || yaml_string_occurrences(payload, "steps.crates-io-auth.outputs.token") != 1
    {
        errors.push(format!(
            "{name} publish step must bind only the short-lived crates.io-auth action output."
        ));
    }
    let run = step
        .get("run")
        .and_then(YamlValue::as_str)
        .unwrap_or_default();
    if run.trim() != "cargo publish -p git-slop --locked --no-verify" {
        errors.push(format!(
            "{name} credentialed publish step must run cargo publish -p git-slop --locked --no-verify exactly."
        ));
    }
    if step.get("id").and_then(YamlValue::as_str) != Some("publish")
        || step.get("if").and_then(YamlValue::as_str) != Some(CRATES_IO_PUBLISH_CONDITION)
        || step.get("continue-on-error").and_then(YamlValue::as_bool) != Some(true)
    {
        errors.push(format!(
            "{name} credentialed publish step must be fail-reconciled and unreachable in recovery mode or when the crate already exists."
        ));
    }
}

fn validate_release_homebrew_token_scope(
    payload: &YamlValue,
    publish_crate: &YamlValue,
    errors: &mut Vec<String>,
) {
    let name = "release-publish.yml";
    let token = "${{ secrets.HOMEBREW_TAP_DISPATCH_TOKEN }}";
    if workflow_or_job_env_contains_value(payload, token) {
        errors.push(format!(
            "{name} must not expose HOMEBREW_TAP_DISPATCH_TOKEN at workflow or job scope."
        ));
    }
    if yaml_string_occurrences(payload, token) != 1 {
        errors.push(format!(
            "{name} must reference the Homebrew dispatch secret exactly once."
        ));
    }
    let token_steps = steps(publish_crate)
        .into_iter()
        .filter(|step| {
            step.get("env")
                .and_then(YamlValue::as_mapping)
                .is_some_and(|env| {
                    env.get(YamlValue::String("GH_TOKEN".into()))
                        .and_then(YamlValue::as_str)
                        == Some(token)
                })
        })
        .collect::<Vec<_>>();
    if token_steps.len() != 1 {
        errors.push(format!(
            "{name} must bind the Homebrew token to exactly one step."
        ));
        return;
    }
    let step = token_steps[0];
    if step.get("name").and_then(YamlValue::as_str)
        != Some("Dispatch immutable release identity to Homebrew tap")
    {
        errors.push(format!(
            "{name} must expose the Homebrew token only to its deliberate immutable-identity dispatch step."
        ));
    }
    for (key, expected) in [
        ("VERSION", "${{ steps.registry.outputs.version }}"),
        ("REVISION", "${{ steps.registry.outputs.revision }}"),
        ("CRATE_URL", "${{ steps.registry.outputs.crate-url }}"),
        ("CRATE_SHA256", "${{ steps.registry.outputs.crate-sha256 }}"),
    ] {
        if step_env(step, key) != Some(expected) {
            errors.push(format!(
                "{name} immutable Homebrew dispatch must bind {key} to {expected}."
            ));
        }
    }
    let run = step
        .get("run")
        .and_then(YamlValue::as_str)
        .unwrap_or_default();
    for required in [
        "test \"$CRATE_URL\" = \"https://static.crates.io/crates/git-slop/git-slop-${VERSION}.crate\"",
        "gh workflow run update-git-slop.yml",
        "--repo coreycoto/homebrew-tap",
        "--ref main",
        "--field version=\"$VERSION\"",
        "--field revision=\"$REVISION\"",
        "--field crate_url=\"$CRATE_URL\"",
        "--field crate_sha256=\"$CRATE_SHA256\"",
    ] {
        require(run, required, name, errors);
    }
    for forbidden in [
        "--field formula_url=",
        "--field formula_sha256=",
        "--field manifest_url=",
        "--field manifest_sha256=",
    ] {
        forbid(run, forbidden, name, errors);
    }
}

fn validate_release_tag_secret_scope(
    payload: &YamlValue,
    publish_crate: &YamlValue,
    errors: &mut Vec<String>,
) {
    let name = "release-publish.yml";
    let secret = "${{ secrets.RELEASE_SIGNING_PRIVATE_KEY }}";
    if workflow_or_job_env_contains_value(payload, secret) {
        errors.push(format!(
            "{name} must not expose RELEASE_SIGNING_PRIVATE_KEY at workflow or job scope."
        ));
    }
    if yaml_string_occurrences(payload, secret) != 1 {
        errors.push(format!(
            "{name} must reference the release signing secret exactly once."
        ));
    }
    let secret_steps = steps(publish_crate)
        .into_iter()
        .filter(|step| step_env(step, "RELEASE_SIGNING_PRIVATE_KEY") == Some(secret))
        .collect::<Vec<_>>();
    if secret_steps.len() != 1 {
        errors.push(format!(
            "{name} must bind the release signing secret to exactly one step."
        ));
        return;
    }
    if secret_steps[0].get("name").and_then(YamlValue::as_str)
        != Some("Create missing exact release tag")
    {
        errors.push(format!(
            "{name} must expose the release signing secret only to the exact tag-creation step."
        ));
    }
}

fn validate_publish_order_and_registry(job: &YamlValue, errors: &mut Vec<String>) {
    let name = "release-publish.yml";
    let names = steps(job)
        .into_iter()
        .map(|step| {
            step.get("name")
                .and_then(YamlValue::as_str)
                .unwrap_or_default()
        })
        .collect::<Vec<_>>();
    let publish = names
        .iter()
        .position(|step| *step == "Publish exact crates.io package");
    let registry = names
        .iter()
        .position(|step| *step == "Download and verify exact registry bytes");
    let tag = names
        .iter()
        .position(|step| *step == "Create missing exact release tag");
    let homebrew = names
        .iter()
        .position(|step| *step == "Dispatch immutable release identity to Homebrew tap");
    if !matches!((publish, registry, tag, homebrew), (Some(publish), Some(registry), Some(tag), Some(homebrew)) if publish < registry && registry < tag && tag < homebrew)
    {
        errors.push(format!(
            "{name} must publish, verify registry bytes, create the exact release tag, and only then dispatch the immutable Homebrew identity."
        ));
    }
    let Some(registry_run) = step_run(job, "Download and verify exact registry bytes") else {
        return;
    };
    for required in [
        "index_sha256=\"$(jq -r '.version.checksum' registry-version.json)\"",
        "test \"$index_sha256\" = \"$EXPECTED_CRATE_SHA256\"",
        "test \"$registry_sha256\" = \"$index_sha256\"",
        "test \"$registry_sha256\" = \"$EXPECTED_CRATE_SHA256\"",
        "cargo xtask verify-crate",
        "cmp ",
        "${CANDIDATE_DIR}/crate-source.json\" registry/crate-source.json",
    ] {
        require(registry_run, required, name, errors);
    }
    let Some(state_run) = step_run(job, "Inspect immutable registry and tag state") else {
        return;
    };
    require(
        state_run,
        "Exact tag exists before crates.io publication; refusing to reverse the release order.",
        name,
        errors,
    );
    require(
        state_run,
        "Recovery requires an existing, non-yanked crates.io package.",
        name,
        errors,
    );
    let Some(tag_run) = step_run(job, "Create missing exact release tag") else {
        return;
    };
    if named_step(job, "Create missing exact release tag")
        .and_then(|step| step_env(step, "CONTROL_REVISION"))
        != Some("${{ needs.candidate.outputs.control-revision }}")
    {
        errors.push(format!(
            "{name} tag mutation must bind the exact trusted workflow control revision."
        ));
    }
    for required in [
        "git fetch --no-tags origin +refs/heads/main:refs/remotes/origin/main",
        "test \"$CONTROL_REVISION\" = \"$GITHUB_SHA\"",
        "test \"$CONTROL_REVISION\" = \"$(git rev-parse refs/remotes/origin/main)\"",
        "git merge-base --is-ancestor \"$REVISION\" refs/remotes/origin/main",
        "test \"$REVISION\" = \"$CONTROL_REVISION\"",
        "test -z \"$(git ls-remote --refs origin \"refs/tags/${TAG}\")\"",
        "push origin \"refs/tags/${TAG}\"",
    ] {
        require(tag_run, required, name, errors);
    }
    for forbidden in [
        "git tag -f",
        "git push --force",
        "git push -f",
        "git push --delete",
        "git tag -d",
    ] {
        forbid(tag_run, forbidden, name, errors);
    }
}

fn validate_release_relay(text: &str, payload: &YamlValue, errors: &mut Vec<String>) {
    let name = "release-published.yml";
    require_exact_trigger(payload, name, "release", errors);
    let release_types = payload
        .get("on")
        .and_then(|on| on.get("release"))
        .and_then(|release| release.get("types"))
        .and_then(YamlValue::as_sequence)
        .map(|types| {
            types
                .iter()
                .filter_map(YamlValue::as_str)
                .collect::<Vec<_>>()
        });
    if release_types != Some(vec!["published"]) {
        errors.push(format!("{name} must run only for release.published."));
    }
    require_permission(payload, name, "contents", "read", errors);
    if payload
        .get("permissions")
        .and_then(|permissions| permissions.get("actions"))
        .is_some()
    {
        errors.push(format!(
            "{name} publication verification must not receive Actions write permission."
        ));
    }
    let Some(jobs) = payload.get("jobs").and_then(YamlValue::as_mapping) else {
        errors.push(format!("{name} must define jobs."));
        return;
    };
    require_exact_job_set(
        jobs,
        name,
        &["verify-publication", "dispatch-scoop"],
        errors,
    );
    let Some(relay) = job(jobs, "verify-publication", name, errors) else {
        return;
    };
    let Some(dispatch_scoop) = job(jobs, "dispatch-scoop", name, errors) else {
        return;
    };
    require_needs(
        dispatch_scoop,
        name,
        "dispatch-scoop",
        &["verify-publication"],
        errors,
    );
    if dispatch_scoop
        .get("permissions")
        .and_then(YamlValue::as_mapping)
        .is_none_or(|permissions| !permissions.is_empty())
    {
        errors.push(format!(
            "{name} dispatch-scoop must receive no same-repository GitHub token permissions."
        ));
    }
    let expected_outputs = [
        ("release-id", "${{ steps.release.outputs.release-id }}"),
        (
            "release-manifest-sha256",
            "${{ steps.release.outputs.release-manifest-sha256 }}",
        ),
        ("revision", "${{ steps.release.outputs.revision }}"),
        ("version", "${{ steps.release.outputs.version }}"),
    ];
    for (key, expected) in expected_outputs {
        if relay
            .get("outputs")
            .and_then(|outputs| outputs.get(key))
            .and_then(YamlValue::as_str)
            != Some(expected)
        {
            errors.push(format!(
                "{name} verify-publication output {key} must bind {expected}."
            ));
        }
    }
    let condition = relay
        .get("if")
        .and_then(YamlValue::as_str)
        .unwrap_or_default();
    if !condition.contains("github.event.release.draft == false")
        || !condition.contains("github.event.release.prerelease == false")
    {
        errors.push(format!(
            "{name} must reject draft and prerelease publication events."
        ));
    }
    let Some(verify) = step_run(
        relay,
        "Verify published release identity and required assets",
    ) else {
        errors.push(format!("{name} must verify the published release."));
        return;
    };
    for required in [
        ".tag_name == $tag and .draft == false and .prerelease == false",
        "release-manifest.json",
        ".schema_version == 3",
        ".crate_source.revision == $revision",
        "sha256:${manifest_sha256}",
        "echo \"release-id=$RELEASE_ID\"",
        "echo \"release-manifest-sha256=$manifest_sha256\"",
        "} >> \"$GITHUB_OUTPUT\"",
    ] {
        require(verify, required, name, errors);
    }
    validate_exact_release_assets(verify, name, "${TAG}", errors);
    if text.contains("gh workflow run homebrew-handoff.yml")
        || text.contains("HOMEBREW_TAP_DISPATCH_TOKEN")
    {
        errors.push(format!(
            "{name} must remain verification-only and must not dispatch Homebrew."
        ));
    }
    validate_scoop_relay_token_scope(payload, dispatch_scoop, errors);
    let Some(summary) = step_run(relay, "Summarize publication verification") else {
        errors.push(format!("{name} must summarize publication verification."));
        return;
    };
    for required in [
        "dispatch-authorized publication job already dispatched",
        "without any Actions environment approval",
        "homebrew-handoff.yml",
        "explicit branch-restricted redispatch",
        "external bucket receiver",
        "manifest-only pull request",
    ] {
        require(summary, required, name, errors);
    }
}

fn validate_scoop_relay_token_scope(
    payload: &YamlValue,
    dispatch_scoop: &YamlValue,
    errors: &mut Vec<String>,
) {
    let name = "release-published.yml";
    let token = "${{ secrets.SCOOP_BUCKET_DISPATCH_TOKEN }}";
    if workflow_or_job_env_contains_value(payload, token) {
        errors.push(format!(
            "{name} must not expose SCOOP_BUCKET_DISPATCH_TOKEN at workflow or job scope."
        ));
    }
    if yaml_string_occurrences(payload, token) != 1 {
        errors.push(format!(
            "{name} must reference the Scoop dispatch secret exactly once."
        ));
    }
    let token_steps = steps(dispatch_scoop)
        .into_iter()
        .filter(|step| step_env(step, "GH_TOKEN") == Some(token))
        .collect::<Vec<_>>();
    if token_steps.len() != 1 {
        errors.push(format!(
            "{name} must bind the Scoop token to exactly one step."
        ));
        return;
    }
    let step = token_steps[0];
    if step.get("name").and_then(YamlValue::as_str)
        != Some("Dispatch immutable release identity to Scoop bucket")
    {
        errors.push(format!(
            "{name} must expose the Scoop token only to its deliberate immutable-identity dispatch step."
        ));
    }
    for (key, expected) in [
        (
            "RELEASE_ID",
            "${{ needs.verify-publication.outputs.release-id }}",
        ),
        (
            "RELEASE_MANIFEST_SHA256",
            "${{ needs.verify-publication.outputs.release-manifest-sha256 }}",
        ),
        (
            "REVISION",
            "${{ needs.verify-publication.outputs.revision }}",
        ),
        ("VERSION", "${{ needs.verify-publication.outputs.version }}"),
    ] {
        if step_env(step, key) != Some(expected) {
            errors.push(format!(
                "{name} immutable Scoop dispatch must bind {key} to {expected}."
            ));
        }
    }
    let run = step
        .get("run")
        .and_then(YamlValue::as_str)
        .unwrap_or_default();
    for required in [
        "gh workflow run update-git-slop.yml",
        "--repo coreycoto/scoop-bucket",
        "--ref main",
        "--field version=\"$VERSION\"",
        "--field revision=\"$REVISION\"",
        "--field release_id=\"$RELEASE_ID\"",
        "--field release_manifest_sha256=\"$RELEASE_MANIFEST_SHA256\"",
    ] {
        require(run, required, name, errors);
    }
    for forbidden in [
        "--field x86_64_sha256=",
        "--field arm64_sha256=",
        "--field asset_url=",
        "--field checksums_url=",
    ] {
        forbid(run, forbidden, name, errors);
    }
}

fn validate_homebrew_handoff(payload: &YamlValue, errors: &mut Vec<String>) {
    let name = "homebrew-handoff.yml";
    require_exact_trigger(payload, name, "workflow_dispatch", errors);
    for input in ["version", "revision"] {
        let valid = payload
            .get("on")
            .and_then(|on| on.get("workflow_dispatch"))
            .and_then(|dispatch| dispatch.get("inputs"))
            .and_then(|inputs| inputs.get(input))
            .is_some_and(|input| {
                input.get("required").and_then(YamlValue::as_bool) == Some(true)
                    && input.get("type").and_then(YamlValue::as_str) == Some("string")
            });
        if !valid {
            errors.push(format!(
                "{name} workflow_dispatch must require string input {input}."
            ));
        }
    }
    require_permission(payload, name, "contents", "read", errors);
    let Some(jobs) = payload.get("jobs").and_then(YamlValue::as_mapping) else {
        errors.push(format!("{name} must define jobs."));
        return;
    };
    require_exact_job_set(jobs, name, &["handoff"], errors);
    let Some(handoff) = job(jobs, "handoff", name, errors) else {
        return;
    };
    require_environment(handoff, name, "handoff", "release", errors);
    let Some(checkout) = steps(handoff).into_iter().find(|step| {
        step.get("name").and_then(YamlValue::as_str) == Some("Checkout trusted current main")
    }) else {
        errors.push(format!("{name} must checkout trusted current main."));
        return;
    };
    if checkout
        .get("with")
        .and_then(|with| with.get("ref"))
        .and_then(YamlValue::as_str)
        != Some("main")
        || checkout
            .get("with")
            .and_then(|with| with.get("persist-credentials"))
            .and_then(YamlValue::as_bool)
            != Some(false)
    {
        errors.push(format!(
            "{name} must checkout main without persisted credentials."
        ));
    }
    let Some(main_check) = step_run(handoff, "Revalidate trusted main for explicit recovery")
    else {
        errors.push(format!(
            "{name} must revalidate trusted main for explicit recovery."
        ));
        return;
    };
    for required in [
        "test \"$GITHUB_REF\" = \"refs/heads/main\"",
        "+refs/heads/main:refs/remotes/origin/main",
        "live_main=\"$(git ls-remote --heads origin refs/heads/main",
        "test \"$(git rev-parse HEAD)\" = \"$GITHUB_SHA\"",
        "test \"$(git rev-parse HEAD)\" = \"$live_main\"",
        "test \"$live_main\" = \"$(git rev-parse refs/remotes/origin/main)\"",
    ] {
        require(main_check, required, name, errors);
    }
    let Some(verify) = step_run(
        handoff,
        "Verify published release, provenance, and every digest",
    ) else {
        errors.push(format!(
            "{name} must verify the complete published release."
        ));
        return;
    };
    for required in [
        ".tag_name == $tag and .draft == false and .prerelease == false",
        "test \"$(wc -l < release-assets/SHA256SUMS | tr -d ' ')\" = \"11\"",
        "sha256sum --check SHA256SUMS",
        ".crate_source.registry == \"crates.io\"",
        "https://static.crates.io/crates/git-slop/git-slop-",
        "(.artifacts | length) == 7",
        "git merge-base --is-ancestor \"$REVISION\" refs/remotes/origin/main",
        "curl --fail --location --retry 5 \"$crate_url\"",
        "test \"$(sha256sum registry.crate | awk '{print $1}')\" = \"$crate_sha256\"",
        "if grep -Eq '^  version[[:space:]]' release-assets/git-slop.rb",
        "assert_match \\\"\\\\\\\"source_revision",
        "assert_match \"\\\"source_dirty",
    ] {
        require(verify, required, name, errors);
    }
    validate_exact_release_assets(verify, name, "${tag}", errors);
    validate_homebrew_token_scope(payload, errors);
}

fn validate_homebrew_token_scope(payload: &YamlValue, errors: &mut Vec<String>) {
    let name = "homebrew-handoff.yml";
    let token = "${{ secrets.HOMEBREW_TAP_DISPATCH_TOKEN }}";
    if workflow_or_job_env_contains_value(payload, token) {
        errors.push(format!(
            "{name} must not expose HOMEBREW_TAP_DISPATCH_TOKEN at workflow or job scope."
        ));
    }
    if yaml_string_occurrences(payload, token) != 1 {
        errors.push(format!(
            "{name} must reference the Homebrew dispatch secret exactly once."
        ));
    }
    let token_steps = workflow_steps(payload)
        .into_iter()
        .filter(|step| {
            step.get("env")
                .and_then(YamlValue::as_mapping)
                .is_some_and(|env| {
                    env.get(YamlValue::String("GH_TOKEN".into()))
                        .and_then(YamlValue::as_str)
                        == Some(token)
                })
        })
        .collect::<Vec<_>>();
    if token_steps.len() != 1 {
        errors.push(format!(
            "{name} must bind the Homebrew token to exactly one step."
        ));
        return;
    }
    let step = token_steps[0];
    if step.get("name").and_then(YamlValue::as_str)
        != Some("Dispatch verified inputs to Homebrew tap")
    {
        errors.push(format!(
            "{name} must expose the Homebrew token only to its deliberate dispatch step."
        ));
    }
    let run = step
        .get("run")
        .and_then(YamlValue::as_str)
        .unwrap_or_default();
    for required in [
        "gh workflow run update-git-slop.yml",
        "--repo coreycoto/homebrew-tap",
        "--ref main",
        "--field version=\"$VERSION\"",
        "--field revision=\"$REVISION\"",
        "--field crate_url=\"$CRATE_URL\"",
        "--field crate_sha256=\"$CRATE_SHA256\"",
    ] {
        require(run, required, name, errors);
    }
    for forbidden in [
        "--field formula_url=",
        "--field formula_sha256=",
        "--field manifest_url=",
        "--field manifest_sha256=",
    ] {
        forbid(run, forbidden, name, errors);
    }
}

fn validate_exact_release_assets(run: &str, name: &str, tag: &str, errors: &mut Vec<String>) {
    let normalized = run
        .split_whitespace()
        .filter(|token| *token != "\\")
        .collect::<Vec<_>>()
        .join(" ");
    let exact = format!(
        "printf '%s\\n' SHA256SUMS git-slop.rb git-slop.cdx.json git-slop.spdx.json \
         \"git-slop-{tag}-aarch64-apple-darwin.tar.gz\" \
         \"git-slop-{tag}-aarch64-pc-windows-msvc.zip\" \
         \"git-slop-{tag}-aarch64-unknown-linux-gnu.tar.gz\" \
         \"git-slop-{tag}-x86_64-apple-darwin.tar.gz\" \
         \"git-slop-{tag}-x86_64-pc-windows-msvc.zip\" \
         \"git-slop-{tag}-x86_64-unknown-linux-gnu.tar.gz\" \
         \"git-slop-{tag}-x86_64-unknown-linux-musl.tar.gz\" \
         release-manifest.json | LC_ALL=C sort > expected-assets.txt"
    );
    if !normalized.contains(&exact) {
        errors.push(format!(
            "{name} must compare the exact twelve release assets against the published inventory."
        ));
    }
    for required in [
        "actual-assets.txt",
        "diff -u expected-assets.txt actual-assets.txt",
    ] {
        require(&normalized, required, name, errors);
    }
}

fn workflow_steps(payload: &YamlValue) -> Vec<&YamlValue> {
    payload
        .get("jobs")
        .and_then(YamlValue::as_mapping)
        .into_iter()
        .flat_map(|jobs| jobs.values())
        .flat_map(steps)
        .collect()
}

fn env_has_key(value: &YamlValue, key: &str) -> bool {
    value
        .get("env")
        .and_then(YamlValue::as_mapping)
        .is_some_and(|env| env.contains_key(YamlValue::String(key.to_owned())))
}

fn workflow_or_job_env_contains(payload: &YamlValue, key: &str) -> bool {
    env_has_key(payload, key)
        || payload
            .get("jobs")
            .and_then(YamlValue::as_mapping)
            .is_some_and(|jobs| jobs.values().any(|job| env_has_key(job, key)))
}

fn workflow_or_job_env_contains_value(payload: &YamlValue, expected: &str) -> bool {
    let contains = |value: &YamlValue| {
        value
            .get("env")
            .and_then(YamlValue::as_mapping)
            .is_some_and(|env| env.values().any(|value| value.as_str() == Some(expected)))
    };
    contains(payload)
        || payload
            .get("jobs")
            .and_then(YamlValue::as_mapping)
            .is_some_and(|jobs| jobs.values().any(contains))
}

fn yaml_string_occurrences(value: &YamlValue, expected: &str) -> usize {
    match value {
        YamlValue::String(value) => value.matches(expected).count(),
        YamlValue::Sequence(values) => values
            .iter()
            .map(|value| yaml_string_occurrences(value, expected))
            .sum(),
        YamlValue::Mapping(values) => values
            .values()
            .map(|value| yaml_string_occurrences(value, expected))
            .sum(),
        _ => 0,
    }
}

fn require_exact_trigger(
    payload: &YamlValue,
    name: &str,
    expected: &str,
    errors: &mut Vec<String>,
) {
    let keys = payload
        .get("on")
        .and_then(YamlValue::as_mapping)
        .map(mapping_keys)
        .unwrap_or_default();
    if keys != BTreeSet::from([expected.to_owned()]) {
        errors.push(format!(
            "{name} must define only the {expected} trigger; found {}.",
            keys.into_iter().collect::<Vec<_>>().join(", ")
        ));
    }
}

fn require_exact_job_set(
    jobs: &serde_yaml::Mapping,
    name: &str,
    expected: &[&str],
    errors: &mut Vec<String>,
) {
    let actual = mapping_keys(jobs);
    let expected = expected
        .iter()
        .map(|value| (*value).to_owned())
        .collect::<BTreeSet<_>>();
    if actual != expected {
        errors.push(format!(
            "{name} job graph does not match the release contract."
        ));
    }
}

fn mapping_keys(mapping: &serde_yaml::Mapping) -> BTreeSet<String> {
    mapping
        .keys()
        .filter_map(YamlValue::as_str)
        .map(str::to_owned)
        .collect()
}

fn job<'a>(
    jobs: &'a serde_yaml::Mapping,
    key: &str,
    name: &str,
    errors: &mut Vec<String>,
) -> Option<&'a YamlValue> {
    match jobs.get(YamlValue::String(key.to_owned())) {
        Some(job) => Some(job),
        None => {
            errors.push(format!("{name} must define job {key}."));
            None
        }
    }
}

fn steps(job: &YamlValue) -> Vec<&YamlValue> {
    job.get("steps")
        .and_then(YamlValue::as_sequence)
        .map(|steps| steps.iter().collect())
        .unwrap_or_default()
}

fn named_step<'a>(job: &'a YamlValue, step_name: &str) -> Option<&'a YamlValue> {
    steps(job)
        .into_iter()
        .find(|step| step.get("name").and_then(YamlValue::as_str) == Some(step_name))
}

fn step_env<'a>(step: &'a YamlValue, key: &str) -> Option<&'a str> {
    step.get("env")
        .and_then(|env| env.get(key))
        .and_then(YamlValue::as_str)
}

fn step_run<'a>(job: &'a YamlValue, step_name: &str) -> Option<&'a str> {
    named_step(job, step_name).and_then(|step| step.get("run").and_then(YamlValue::as_str))
}

fn require_needs(
    job: &YamlValue,
    name: &str,
    job_name: &str,
    expected: &[&str],
    errors: &mut Vec<String>,
) {
    let actual = match job.get("needs") {
        Some(YamlValue::String(value)) => BTreeSet::from([value.to_owned()]),
        Some(YamlValue::Sequence(values)) => values
            .iter()
            .filter_map(YamlValue::as_str)
            .map(str::to_owned)
            .collect(),
        _ => BTreeSet::new(),
    };
    let expected = expected
        .iter()
        .map(|value| (*value).to_owned())
        .collect::<BTreeSet<_>>();
    if actual != expected {
        errors.push(format!(
            "{name} {job_name} needs do not match the protected release order."
        ));
    }
}

fn require_environment(
    job: &YamlValue,
    name: &str,
    job_name: &str,
    expected: &str,
    errors: &mut Vec<String>,
) {
    if job.get("environment").and_then(YamlValue::as_str) != Some(expected) {
        errors.push(format!(
            "{name} {job_name} must use the required {expected} environment."
        ));
    }
}

fn require_exact_job_permission(
    job: &YamlValue,
    name: &str,
    job_name: &str,
    permission: &str,
    expected: &str,
    errors: &mut Vec<String>,
) {
    let Some(permissions) = job.get("permissions").and_then(YamlValue::as_mapping) else {
        errors.push(format!(
            "{name} {job_name} must grant only {permission}: {expected}."
        ));
        return;
    };
    if permissions.len() != 1
        || permissions.get(permission).and_then(YamlValue::as_str) != Some(expected)
    {
        errors.push(format!(
            "{name} {job_name} must grant only {permission}: {expected}."
        ));
    }
}

fn require_permission(
    payload: &YamlValue,
    name: &str,
    permission: &str,
    expected: &str,
    errors: &mut Vec<String>,
) {
    if payload
        .get("permissions")
        .and_then(|permissions| permissions.get(permission))
        .and_then(YamlValue::as_str)
        != Some(expected)
    {
        errors.push(format!(
            "{name} must grant {permission}: {expected} at workflow scope."
        ));
    }
}

fn validate_target_matrix(
    job: &YamlValue,
    name: &str,
    job_name: &str,
    require_archive: bool,
    errors: &mut Vec<String>,
) {
    let includes = job
        .get("strategy")
        .and_then(|strategy| strategy.get("matrix"))
        .and_then(|matrix| matrix.get("include"))
        .and_then(YamlValue::as_sequence);
    let Some(includes) = includes else {
        errors.push(format!(
            "{name} {job_name} must define an exact target matrix."
        ));
        return;
    };
    let targets = includes
        .iter()
        .filter_map(|entry| entry.get("target").and_then(YamlValue::as_str))
        .collect::<BTreeSet<_>>();
    if includes.len() != RELEASE_TARGETS.len()
        || targets != RELEASE_TARGETS.into_iter().collect::<BTreeSet<_>>()
    {
        errors.push(format!(
            "{name} {job_name} must contain exactly the seven supported targets."
        ));
    }
    let runner_targets = includes
        .iter()
        .filter_map(|entry| Some((entry.get("os")?.as_str()?, entry.get("target")?.as_str()?)))
        .collect::<BTreeSet<_>>();
    if runner_targets != RELEASE_TARGET_RUNNERS.into_iter().collect::<BTreeSet<_>>() {
        errors.push(format!(
            "{name} {job_name} must bind each supported target to its exact runner."
        ));
    }
    if job.get("runs-on").and_then(YamlValue::as_str) != Some("${{ matrix.os }}") {
        errors.push(format!(
            "{name} {job_name} must run each target on matrix.os."
        ));
    }
    for entry in includes {
        if entry.get("os").and_then(YamlValue::as_str).is_none()
            || (require_archive && entry.get("archive").and_then(YamlValue::as_str).is_none())
        {
            errors.push(format!(
                "{name} {job_name} matrix entries must include runner metadata."
            ));
        }
    }
}

fn validate_agent_plugin_runtime(workflows: &Path, errors: &mut Vec<String>) {
    for name in AGENT_PLUGIN_WORKFLOWS {
        let Some(text) = read(&workflows.join(name), errors) else {
            continue;
        };
        for required in [
            PREPARE_COMMAND,
            VERIFY_COMMAND,
            "AGENT_PLUGINS_READ_TOKEN: ${{ secrets.AGENT_PLUGINS_READ_TOKEN }}",
        ] {
            require(&text, required, name, errors);
        }
        for forbidden in [
            "actions/setup-python",
            "python-version:",
            "python -m pip",
            "pip install",
            "Install uv",
            "uv run",
            "uv sync",
            "AGENT_PLUGINS_GIT_TOKEN",
            "python -m agent_plugins",
            "python -c \"from agent_plugins",
            "actions/cache@",
            "RUNNER_TOOL_CACHE",
            "runner.tool_cache",
            "restore-keys:",
        ] {
            forbid(&text, forbidden, name, errors);
        }
    }

    if let Some(text) = read(&workflows.join("execution_state_sync.yml"), errors) {
        require(
            &text,
            "scripts/with-agent-plugins.sh github project-snapshot",
            "execution_state_sync.yml",
            errors,
        );
        require(
            &text,
            "scripts/with-agent-plugins.sh github execution-state",
            "execution_state_sync.yml",
            errors,
        );
    }

    for name in PUBLIC_RELEASE_WORKFLOWS {
        if let Some(text) = read(&workflows.join(name), errors) {
            validate_no_private_runtime(name, &text, errors);
        }
    }
}

fn validate_action_versions(repo_root: &Path, workflows: &Path, errors: &mut Vec<String>) {
    let mut surfaces = vec![repo_root.join("action.yml")];
    if let Ok(entries) = fs::read_dir(workflows) {
        surfaces.extend(entries.flatten().map(|entry| entry.path()).filter(|path| {
            path.extension().and_then(|extension| extension.to_str()) == Some("yml")
        }));
    }
    for path in surfaces {
        let Some(text) = read(&path, errors) else {
            continue;
        };
        let label = relative(repo_root, &path);
        forbid(&text, "actions/upload-artifact@v4", &label, errors);
        forbid(&text, "actions/upload-artifact@v5", &label, errors);
    }
}

fn validate_artifacts(workflows: &Path, errors: &mut Vec<String>) {
    let contracts: [(&str, &[&str]); 4] = [
        (
            "dependency-remediation.yml",
            &[
                ".artifacts/codex/dependency-remediation.json",
                ".artifacts/dependency-remediation/",
            ],
        ),
        (
            "docs-taxonomy.yml",
            &[
                ".artifacts/codex/docs-taxonomy.json",
                ".artifacts/docs-taxonomy/",
            ],
        ),
        (
            "governance-reconcile.yml",
            &[
                ".artifacts/codex/governance-reconcile.json",
                ".artifacts/github-governance/",
            ],
        ),
        (
            "merge-on-green.yml",
            &[".artifacts/codex/merge-on-green.json"],
        ),
    ];

    for (name, expected_paths) in contracts {
        let Some(text) = read(&workflows.join(name), errors) else {
            continue;
        };
        let upload = text
            .split_once("      - name: Upload ")
            .map(|(_, tail)| tail);
        let Some(upload) = upload else {
            errors.push(format!("{name} must define an upload step."));
            continue;
        };
        for expected in [
            "steps.codex_preflight.outputs.enabled == 'true'",
            "always()",
            "include-hidden-files: true",
            "if-no-files-found: error",
            "retention-days: 14",
        ] {
            require(upload, expected, name, errors);
        }
        forbid(upload, "          path: .artifacts\n", name, errors);
        for path in expected_paths {
            require(upload, path, name, errors);
        }
        if name == "merge-on-green.yml" {
            require(
                upload,
                "steps.merge_preflight.outputs.eligible == 'true'",
                name,
                errors,
            );
        }
    }

    if let Some(text) = read(&workflows.join("execution_state_sync.yml"), errors) {
        validate_execution_state_artifacts(&text, errors);
    }
}

fn validate_execution_state_artifacts(text: &str, errors: &mut Vec<String>) {
    let name = "execution_state_sync.yml";
    let artifact_root = text.find("      - name: Prepare artifact root");
    let runtime_prepare = text.find("      - name: Prepare pinned agent-plugins runtime");
    if !matches!((artifact_root, runtime_prepare), (Some(root), Some(runtime)) if root < runtime) {
        errors.push(format!(
            "{name} must create its artifact root before private runtime preparation."
        ));
    }

    let upload = text
        .split_once("      - name: Upload execution artifacts")
        .map(|(_, tail)| tail);
    let Some(upload) = upload else {
        errors.push(format!("{name} must define its artifact upload."));
        return;
    };
    for expected in [
        "if: ${{ (failure() || github.event_name == 'workflow_dispatch') && steps.artifact-root.outputs.path != '' }}",
        "path: ${{ steps.artifact-root.outputs.path }}",
        "include-hidden-files: true",
        "if-no-files-found: error",
        "retention-days: 14",
    ] {
        require(upload, expected, name, errors);
    }
}

fn validate_dogfood(workflows: &Path, errors: &mut Vec<String>) {
    let name = "dogfood.yml";
    let Some(text) = read(&workflows.join(name), errors) else {
        return;
    };
    for expected in [
        "cargo build -p git-slop --release --locked",
        "target/release/git-slop find",
        "cat .slop/latest/health.md",
        "path: .slop/latest/health.md",
        "include-hidden-files: true",
        "retention-days: 14",
    ] {
        require(&text, expected, name, errors);
    }
    forbid(&text, "path: .slop/latest\n", name, errors);
    forbid(&text, "uv run git-slop", name, errors);
}

fn validate_ci(repo_root: &Path, workflows: &Path, errors: &mut Vec<String>) {
    let name = "ci.yml";
    let Some(text) = read(&workflows.join(name), errors) else {
        return;
    };
    for expected in [
        "cargo fmt -p git-slop -- --check",
        "cargo clippy -p git-slop --all-targets --all-features --locked",
        "cargo test -p git-slop --all-targets --all-features --locked",
        "cargo fmt --manifest-path xtask/Cargo.toml --all -- --check",
        "cargo clippy --manifest-path xtask/Cargo.toml --all-targets --all-features --locked",
        "cargo test --manifest-path xtask/Cargo.toml --all-targets --all-features --locked",
        "cargo package -p git-slop --locked",
        "cargo publish -p git-slop --dry-run --locked",
        "cargo xtask validate",
        "node --test action/*.test.mjs",
        "ubuntu-24.04",
        "macos-15",
        "windows-2025",
        "windows-11-arm",
    ] {
        require(&text, expected, name, errors);
    }
    for forbidden in [
        "maintainer-tooling:",
        "Python maintainer tooling",
        "uv sync",
        "uv run pytest",
        "scripts/smoke_plugin_consumer.py",
        "tests/unit/agent_tools",
        "python -m git_slop",
        "macos-15-intel",
        "uv build",
    ] {
        forbid(&text, forbidden, name, errors);
    }
    validate_runtime_launcher_ci_job(&text, name, errors);
    validate_windows_action_ci_job(&text, name, errors);
    validate_runtime_launcher_fixture(repo_root, errors);
}

fn validate_runtime_launcher_ci_job(text: &str, name: &str, errors: &mut Vec<String>) {
    const COMMAND: &str = "bash scripts/with-agent-plugins.test.sh";
    let payload = match serde_yaml::from_str::<YamlValue>(text) {
        Ok(payload) => payload,
        Err(error) => {
            errors.push(format!("Unable to parse {name}: {error}"));
            return;
        }
    };
    let command_is_in_rust_quality = payload
        .get("jobs")
        .and_then(|jobs| jobs.get("rust-quality"))
        .and_then(|job| job.get("steps"))
        .and_then(YamlValue::as_sequence)
        .is_some_and(|steps| {
            steps.iter().any(|step| {
                step.get("run")
                    .and_then(YamlValue::as_str)
                    .is_some_and(|run| run.trim() == COMMAND)
            })
        });
    if !command_is_in_rust_quality {
        errors.push(format!("{name} rust-quality job must run {COMMAND}."));
    }
}

fn validate_windows_action_ci_job(text: &str, name: &str, errors: &mut Vec<String>) {
    const PLATFORM_OSES: [&str; 4] = ["ubuntu-24.04", "macos-15", "windows-2025", "windows-11-arm"];
    const SETUP_STEP: &str = "Set up Node.js for Windows Action tests";
    const TEST_STEP: &str = "Test GitHub Action on Windows";
    const WINDOWS_CONDITION: &str = "runner.os == 'Windows'";
    const TEST_COMMAND: &str = "node --test action/install.test.mjs";

    let payload = match serde_yaml::from_str::<YamlValue>(text) {
        Ok(payload) => payload,
        Err(error) => {
            errors.push(format!("Unable to parse {name}: {error}"));
            return;
        }
    };
    let Some(jobs) = payload.get("jobs").and_then(YamlValue::as_mapping) else {
        errors.push(format!("{name} must define jobs."));
        return;
    };
    let Some(platform_smoke) = job(jobs, "platform-smoke", name, errors) else {
        return;
    };

    if platform_smoke.get("runs-on").and_then(YamlValue::as_str) != Some("${{ matrix.os }}") {
        errors.push(format!(
            "{name} platform-smoke job must use matrix.os as runs-on."
        ));
    }
    let matrix = platform_smoke
        .get("strategy")
        .and_then(|strategy| strategy.get("matrix"));
    let platform_oses = matrix
        .and_then(|matrix| matrix.get("os"))
        .and_then(YamlValue::as_sequence)
        .map(|oses| {
            oses.iter()
                .filter_map(YamlValue::as_str)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    if platform_oses.as_slice() != PLATFORM_OSES.as_slice() {
        errors.push(format!(
            "{name} platform-smoke job must define the exact supported platform matrix."
        ));
    }
    if matrix
        .and_then(|matrix| matrix.get("exclude"))
        .and_then(YamlValue::as_sequence)
        .is_some_and(|excludes| {
            excludes.iter().any(|exclude| {
                exclude
                    .get("os")
                    .and_then(YamlValue::as_str)
                    .is_some_and(|os| matches!(os, "windows-2025" | "windows-11-arm"))
            })
        })
    {
        errors.push(format!(
            "{name} platform-smoke job must not exclude either supported Windows lane."
        ));
    }

    let platform_steps = steps(platform_smoke);

    for step_name in [SETUP_STEP, TEST_STEP] {
        let count = platform_steps
            .iter()
            .filter(|step| step.get("name").and_then(YamlValue::as_str) == Some(step_name))
            .count();
        if count != 1 {
            errors.push(format!(
                "{name} platform-smoke job must define exactly one {step_name} step."
            ));
        }
    }

    let setup = named_step(platform_smoke, SETUP_STEP);
    let test = named_step(platform_smoke, TEST_STEP);

    if let Some(setup) = setup {
        if setup.get("if").and_then(YamlValue::as_str) != Some(WINDOWS_CONDITION) {
            errors.push(format!(
                "{name} {SETUP_STEP} step must use the exact Windows runner condition."
            ));
        }
        if setup.get("uses").and_then(YamlValue::as_str) != Some("actions/setup-node@v7") {
            errors.push(format!(
                "{name} {SETUP_STEP} step must use actions/setup-node@v7."
            ));
        }
        if setup
            .get("with")
            .and_then(|with| with.get("node-version"))
            .and_then(YamlValue::as_str)
            != Some("24")
        {
            errors.push(format!("{name} {SETUP_STEP} step must install Node.js 24."));
        }
    }

    if let Some(test) = test {
        if test.get("if").and_then(YamlValue::as_str) != Some(WINDOWS_CONDITION) {
            errors.push(format!(
                "{name} {TEST_STEP} step must use the exact Windows runner condition."
            ));
        }
        if test.get("run").and_then(YamlValue::as_str).map(str::trim) != Some(TEST_COMMAND) {
            errors.push(format!(
                "{name} {TEST_STEP} step must run exactly {TEST_COMMAND}."
            ));
        }
    }

    if setup.is_some() && test.is_some() {
        let setup_position = platform_steps
            .iter()
            .position(|step| step.get("name").and_then(YamlValue::as_str) == Some(SETUP_STEP))
            .expect("named setup step must have a position");
        let test_position = platform_steps
            .iter()
            .position(|step| step.get("name").and_then(YamlValue::as_str) == Some(TEST_STEP))
            .expect("named test step must have a position");
        if setup_position >= test_position {
            errors.push(format!(
                "{name} {SETUP_STEP} step must run before {TEST_STEP}."
            ));
        }
    }
}

fn validate_runtime_launcher_fixture(repo_root: &Path, errors: &mut Vec<String>) {
    let relative = "scripts/with-agent-plugins.test.sh";
    let path = repo_root.join(relative);
    let Ok(metadata) = fs::symlink_metadata(&path) else {
        errors.push(format!(
            "{relative} must exist as a regular executable file."
        ));
        return;
    };
    if !metadata.is_file() {
        errors.push(format!("{relative} must be a regular file."));
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        if metadata.permissions().mode() & 0o111 == 0 {
            errors.push(format!(
                "{relative} must be executable as part of the runtime-launcher test contract."
            ));
        }
    }
}

fn read(path: &Path, errors: &mut Vec<String>) -> Option<String> {
    match fs::read_to_string(path) {
        Ok(text) => Some(text),
        Err(error) => {
            errors.push(format!("Unable to read {}: {error}", path.display()));
            None
        }
    }
}

fn require(text: &str, expected: &str, label: &str, errors: &mut Vec<String>) {
    if !text.contains(expected) {
        errors.push(format!("{label} must include {expected}."));
    }
}

fn forbid(text: &str, forbidden: &str, label: &str, errors: &mut Vec<String>) {
    if text.contains(forbidden) {
        errors.push(format!("{label} must not include {forbidden}."));
    }
}

fn relative(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn workflow_text(name: &str) -> String {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
        fs::read_to_string(root.join(".github/workflows").join(name)).unwrap()
    }

    fn parsed(text: &str) -> YamlValue {
        serde_yaml::from_str(text).unwrap()
    }

    fn publish_errors(text: &str) -> Vec<String> {
        let mut errors = Vec::new();
        validate_release_publish(text, &parsed(text), &mut errors);
        errors
    }

    fn relay_errors(text: &str) -> Vec<String> {
        let mut errors = Vec::new();
        validate_release_relay(text, &parsed(text), &mut errors);
        errors
    }

    fn homebrew_errors(text: &str) -> Vec<String> {
        let mut errors = Vec::new();
        validate_homebrew_handoff(&parsed(text), &mut errors);
        errors
    }

    #[test]
    fn repository_workflows_pass() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
        assert_eq!(validate(root), Vec::<String>::new());
    }

    #[test]
    fn release_publish_contract_rejects_boundary_regressions() {
        let valid = workflow_text("release-publish.yml");
        assert_eq!(publish_errors(&valid), Vec::<String>::new());

        let cases = [
            (
                valid.replacen(
                    RELEASE_DISPATCH_AUTHORIZATION,
                    "Publish exact current main, or recover an already-published immutable crate.",
                    1,
                ),
                "explicit publication authorization",
            ),
            (
                valid.replacen(
                    CRATES_IO_RELEASE_USER_AGENT,
                    r#"--user-agent "curl/8""#,
                    1,
                ),
                "identify every crates.io API request",
            ),
            (
                valid.replacen("environment: release", "environment: unprotected", 1),
                "required release environment",
            ),
            (
                valid.replacen(
                    "name: Dispatch-authorized crates.io publication and exact tag",
                    "name: Publish crates.io",
                    1,
                ),
                "dispatch-authorized publication boundary",
            ),
            (
                valid.replacen("adds no reviewer gate", "requires a reviewer", 1),
                "adds no reviewer gate",
            ),
            (
                valid.replacen(
                    "cargo publish -p git-slop --locked --no-verify",
                    "cargo publish -p git-slop --locked",
                    1,
                ),
                "--no-verify exactly",
            ),
            (
                valid.replacen(
                    "test \"$index_sha256\" = \"$EXPECTED_CRATE_SHA256\"",
                    "true",
                    1,
                ),
                "index_sha256",
            ),
            (
                valid.replacen(
                    "- name: Create missing exact release tag",
                    "- name: Create release tag early",
                    1,
                ),
                "create the exact release tag",
            ),
            (
                valid.replacen(
                    "target: aarch64-pc-windows-msvc",
                    "target: unsupported-target",
                    1,
                ),
                "exactly the seven supported targets",
            ),
            (
                valid.replacen(
                    r#"-c user.name="git-slop release validation""#,
                    r#"-c user.name="""#,
                    1,
                ),
                "git-slop release validation",
            ),
            (
                valid.replacen(
                    r#"-c user.email="actions@users.noreply.github.com""#,
                    r#"-c user.email="""#,
                    1,
                ),
                "actions@users.noreply.github.com",
            ),
            (
                valid.replacen(
                    "sha256sum git-slop.rb >> SHA256SUMS",
                    "sha256sum git-slop.rb release-manifest.json >> SHA256SUMS",
                    1,
                ),
                "must not include sha256sum git-slop.rb release-manifest.json",
            ),
            (
                valid.replacen(
                    "brew audit --strict --formula",
                    "brew audit --formula",
                    1,
                ),
                "must include brew audit --strict --formula",
            ),
            (
                valid.replacen(
                    "Homebrew/actions/setup-homebrew@df4b09108a1de9d6f995fe68f302b3f68bd6d2ef",
                    "Homebrew/actions/setup-homebrew@main",
                    1,
                ),
                "must use the pinned Homebrew setup Action",
            ),
            (
                valid.replacen(
                    "      - name: Download candidate Formula\n        uses: actions/download-artifact@3e5f45b2cfb9172054b4087a40e8e0b5a5461e7c # v8\n        with:\n          name: candidate-homebrew-formula",
                    "      - name: Download candidate Formula\n        uses: actions/download-artifact@3e5f45b2cfb9172054b4087a40e8e0b5a5461e7c # v8\n        with:\n          name: untrusted-formula",
                    1,
                ),
                "must download only the generated Formula with the pinned artifact contract",
            ),
            (
                valid.replacen(
                    "          path: candidate-dist/git-slop.rb\n          if-no-files-found: error\n          retention-days: 1",
                    "          path: candidate-dist\n          if-no-files-found: warn\n          retention-days: 14",
                    1,
                ),
                "must upload only the generated Formula with the pinned bounded artifact contract",
            ),
            (
                valid.replacen(
                    "needs: [candidate, candidate-distribution, candidate-homebrew-audit]",
                    "needs: [candidate, candidate-distribution]",
                    1,
                ),
                "publish-crate needs do not match the protected release order",
            ),
            (
                valid.replacen(
                    r#"test "$ACTUAL_CRATE_SHA256" = "$EXPECTED_CRATE_SHA256""#,
                    "true",
                    1,
                ),
                "ACTUAL_CRATE_SHA256",
            ),
            (
                valid.replacen("        id: release-install", "        id: loose-install", 1),
                "stable release-install step id",
            ),
            (
                valid.replacen(
                    "          ACTUAL_MANIFEST_SHA256: ${{ steps.release-install.outputs.release-manifest-sha256 }}",
                    "          ACTUAL_MANIFEST_SHA256: untrusted",
                    1,
                ),
                "ACTUAL_MANIFEST_SHA256",
            ),
            (
                valid.replacen(
                    "          EXPECTED_CRATE_SHA256: ${{ needs.publish-crate.outputs.crate-sha256 }}",
                    "          EXPECTED_CRATE_SHA256: untrusted",
                    1,
                ),
                "published crate digest",
            ),
            (
                valid.replacen(
                    "release-id: ${{ steps.release.outputs.release-id || steps.draft.outputs.release-id }}",
                    "release-id: untrusted",
                    1,
                ),
                "resolved numeric release ID",
            ),
            (
                valid.replacen(
                    "release-manifest-sha256: ${{ steps.release-install.outputs.release-manifest-sha256 }}",
                    "release-manifest-sha256: untrusted",
                    1,
                ),
                "verified release manifest digest",
            ),
            (
                valid.replacen(
                    "asset-sha256-by-target: ${{ steps.release-identity.outputs.asset-sha256-by-target }}",
                    "asset-sha256-by-target: untrusted",
                    1,
                ),
                "verified archive digests by target",
            ),
            (
                valid.replacen(
                    "gh release create \"$TAG\" --draft --generate-notes --title \"$TAG\" --verify-tag",
                    "gh release create \"$TAG\" --draft --generate-notes --title \"$TAG\"",
                    1,
                ),
                "--verify-tag",
            ),
            (
                valid.replacen(
                    "Multiple GitHub Releases use exact tag ${TAG}; refusing ambiguous release mutation.",
                    "Ignoring duplicate exact-tag releases.",
                    1,
                ),
                "Multiple GitHub Releases use exact tag",
            ),
            (
                valid.replacen(
                    "https://uploads.github.com/repos/${GITHUB_REPOSITORY}/releases/${release_id}/assets?name=${asset_name}",
                    "https://uploads.github.com/repos/${GITHUB_REPOSITORY}/releases/tags/${TAG}/assets?name=${asset_name}",
                    1,
                ),
                "uploads.github.com/repos/${GITHUB_REPOSITORY}/releases/${release_id}/assets?name=${asset_name}",
            ),
            (
                valid.replacen(
                    "test \"$(jq -r '.[0].id' release-matches.json)\" = \"$RELEASE_ID\"",
                    "true",
                    1,
                ),
                "release-matches.json)\" = \"$RELEASE_ID",
            ),
            (
                valid.replacen(
                    "          ref: ${{ needs.publish-crate.outputs.control-revision }}\n          path: release-control",
                    "          ref: ${{ needs.publish-crate.outputs.revision }}\n          path: release-control",
                    1,
                ),
                "trusted current-main revision",
            ),
            (
                valid.replacen(
                    "          ACTION_INSTALLER: ${{ needs.publish-crate.outputs.mode == 'recover' && 'release-control/action/install.mjs' || 'action/install.mjs' }}",
                    "          ACTION_INSTALLER: action/install.mjs",
                    1,
                ),
                "ACTION_INSTALLER",
            ),
            (
                valid.replacen(
                    "      contents: write\n    env:\n      VERSION: ${{ needs.publish-crate.outputs.version }}\n      TAG: ${{ needs.publish-crate.outputs.tag }}\n      REVISION: ${{ needs.publish-crate.outputs.revision }}",
                    "      contents: read\n    env:\n      VERSION: ${{ needs.publish-crate.outputs.version }}\n      TAG: ${{ needs.publish-crate.outputs.tag }}\n      REVISION: ${{ needs.publish-crate.outputs.revision }}",
                    1,
                ),
                "draft-action-smoke must grant only contents: write",
            ),
            (
                valid.replacen(
                    "          - os: windows-11-arm\n            target: aarch64-pc-windows-msvc",
                    "          - os: windows-2025\n            target: aarch64-pc-windows-msvc",
                    1,
                ),
                "exact runner",
            ),
            (
                valid.replacen(
                    "    runs-on: ${{ matrix.os }}",
                    "    runs-on: ubuntu-24.04",
                    1,
                ),
                "run each target on matrix.os",
            ),
            (
                valid.replacen(
                    "      contents: write\n    env:\n      VERSION: ${{ needs.publish-crate.outputs.version }}\n      TAG: ${{ needs.publish-crate.outputs.tag }}\n      REVISION: ${{ needs.publish-crate.outputs.revision }}",
                    "      contents: write\n      issues: write\n    env:\n      VERSION: ${{ needs.publish-crate.outputs.version }}\n      TAG: ${{ needs.publish-crate.outputs.tag }}\n      REVISION: ${{ needs.publish-crate.outputs.revision }}",
                    1,
                ),
                "draft-action-smoke must grant only contents: write",
            ),
            (
                valid.replacen(
                    "          github-token: ${{ github.token }}",
                    "          github-token: untrusted",
                    1,
                ),
                "composite Action input github-token",
            ),
            (
                valid.replacen(
                    "          git fetch --no-tags origin \"refs/tags/${TAG}:refs/tags/${TAG}\"\n          test \"$(git rev-parse \"refs/tags/${TAG}^{commit}\")\" = \"$REVISION\"\n          test -z \"$(git status --short)\"",
                    "          git fetch --no-tags origin \"refs/tags/${TAG}:refs/tags/${TAG}\"\n          test \"$(git rev-parse \"refs/tags/${TAG}^{commit}\")\" = \"$REVISION\"\n          test -z \"$(git status --short)\"\n          printf '%s' \"${{ github.token }}\" >/dev/null",
                    1,
                ),
                "pass github.token only through the composite Action github-token input",
            ),
            (
                valid.replacen(
                    "env:\n  CARGO_TERM_COLOR: always",
                    "env:\n  CARGO_TERM_COLOR: always\n  GIT_SLOP_GITHUB_TOKEN: ${{ github.token }}",
                    1,
                ),
                "must not expose GIT_SLOP_GITHUB_TOKEN at workflow or job scope",
            ),
            (
                valid.replacen(
                    "      - name: Checkout exact release Action revision\n        uses: actions/checkout@3d3c42e5aac5ba805825da76410c181273ba90b1 # v7\n        with:\n          ref: ${{ needs.publish-crate.outputs.revision }}",
                    "      - name: Checkout exact release Action revision\n        uses: actions/checkout@3d3c42e5aac5ba805825da76410c181273ba90b1 # v7\n        with:\n          ref: ${{ needs.publish-crate.outputs.control-revision }}",
                    1,
                ),
                "exact release revision with tag history",
            ),
            (
                valid.replacen(
                    "      - name: Checkout exact release Action revision\n        uses: actions/checkout@3d3c42e5aac5ba805825da76410c181273ba90b1 # v7",
                    "      - name: Checkout exact release Action revision\n        uses: actions/checkout@v7",
                    1,
                ),
                "exact release revision with tag history",
            ),
            (
                valid.replacen(
                    "      - name: Checkout exact release Action revision\n        uses: actions/checkout@3d3c42e5aac5ba805825da76410c181273ba90b1 # v7\n        with:\n          ref: ${{ needs.publish-crate.outputs.revision }}\n          fetch-depth: 0\n          persist-credentials: false",
                    "      - name: Checkout exact release Action revision\n        uses: actions/checkout@3d3c42e5aac5ba805825da76410c181273ba90b1 # v7\n        with:\n          ref: ${{ needs.publish-crate.outputs.revision }}\n          fetch-depth: 0\n          persist-credentials: true",
                    1,
                ),
                "without persisted credentials",
            ),
            (
                valid.replacen("        uses: ./", "        uses: ./action", 1),
                "exact checked-out composite Action",
            ),
            (
                valid.replacen(
                    "      - name: Run exact release composite Action\n        id: git-slop",
                    "      - name: Run exact release composite Action\n        if: false\n        id: git-slop",
                    1,
                ),
                "Run exact release composite Action must execute unconditionally and fail closed",
            ),
            (
                valid.replacen(
                    "  draft-action-smoke:\n    name: Draft Action smoke on ${{ matrix.target }}",
                    "  draft-action-smoke:\n    name: Draft Action smoke on ${{ matrix.target }}\n    continue-on-error: true",
                    1,
                ),
                "draft-action-smoke must not be conditional or fail open",
            ),
            (
                valid.replacen(
                    "          GIT_SLOP_RELEASE_ID: ${{ needs.draft-release.outputs.release-id }}",
                    "          GIT_SLOP_RELEASE_ID: untrusted",
                    1,
                ),
                "GIT_SLOP_RELEASE_ID",
            ),
            (
                valid.replacen(
                    "          EXPECTED_MANIFEST_SHA256: ${{ needs.draft-release.outputs.release-manifest-sha256 }}",
                    "          EXPECTED_MANIFEST_SHA256: untrusted",
                    1,
                ),
                "EXPECTED_MANIFEST_SHA256",
            ),
            (
                valid.replacen(
                    "          EXPECTED_TARGET: ${{ matrix.target }}",
                    "          EXPECTED_TARGET: x86_64-unknown-linux-gnu",
                    1,
                ),
                "EXPECTED_TARGET",
            ),
            (
                valid.replacen(
                    "          STATUS: ${{ steps.git-slop.outputs.status }}",
                    "          STATUS: advisory",
                    1,
                ),
                "STATUS",
            ),
            (
                valid.replacen(
                    r#"test "$ACTUAL_ASSET_SHA256" = "$EXPECTED_ASSET_SHA256""#,
                    r#"[[ "$ACTUAL_ASSET_SHA256" =~ ^[0-9a-f]{64}$ ]]"#,
                    1,
                ),
                "ACTUAL_ASSET_SHA256",
            ),
            (
                valid.replacen(
                    "  marketplace-ready:\n    name: Marketplace release ready",
                    "  marketplace-ready:\n    name: Marketplace release ready\n    if: always()",
                    1,
                ),
                "marketplace-ready must depend normally on successful smoke and fail closed",
            ),
            (
                valid.replacen(
                    "          CRATE_SHA256: ${{ steps.registry.outputs.crate-sha256 }}",
                    "          CRATE_SHA256: ${{ needs.candidate.outputs.crate-sha256 }}",
                    1,
                ),
                "immutable Homebrew dispatch must bind CRATE_SHA256",
            ),
        ];
        for (drifted, expected) in cases {
            assert_ne!(drifted, valid, "mutation fixture did not match: {expected}");
            let errors = publish_errors(&drifted).join("\n");
            assert!(errors.contains(expected), "missing {expected}: {errors}");
        }

        let job_scoped_token = valid.replacen(
            "    environment: release\n    permissions:",
            "    environment: release\n    env:\n      CARGO_REGISTRY_TOKEN: ${{ secrets.CARGO_REGISTRY_TOKEN }}\n    permissions:",
            1,
        );
        let errors = publish_errors(&job_scoped_token).join("\n");
        assert!(errors.contains("workflow or job scope"), "{errors}");

        let job_scoped_homebrew_token = valid.replacen(
            "    environment: release\n    permissions:",
            "    environment: release\n    env:\n      HOMEBREW_TOKEN: ${{ secrets.HOMEBREW_TAP_DISPATCH_TOKEN }}\n    permissions:",
            1,
        );
        let errors = publish_errors(&job_scoped_homebrew_token).join("\n");
        assert!(errors.contains("workflow or job scope"), "{errors}");
    }

    #[test]
    fn release_publish_trusted_publishing_contract_rejects_auth_regressions() {
        let valid = workflow_text("release-publish.yml");
        assert_eq!(publish_errors(&valid), Vec::<String>::new());

        let cases = [
            (
                valid.replacen(
                    "    environment: release\n    permissions:\n      contents: write\n      id-token: write\n    outputs:",
                    "    environment: release\n    permissions:\n      contents: write\n    outputs:",
                    1,
                ),
                "grant exactly contents: write and id-token: write",
            ),
            (
                valid.replacen(
                    "      contents: write\n      id-token: write\n    outputs:",
                    "      contents: write\n      id-token: write\n      packages: write\n    outputs:",
                    1,
                ),
                "grant exactly contents: write and id-token: write",
            ),
            (
                valid.replacen(
                    "env:\n  CARGO_TERM_COLOR: always",
                    "permissions:\n  id-token: write\n\nenv:\n  CARGO_TERM_COLOR: always",
                    1,
                ),
                "must not grant id-token permission at workflow scope",
            ),
            (
                valid.replacen(
                    "  candidate:\n    name: Validate exact release identity\n    runs-on: ubuntu-24.04\n    permissions:\n      contents: read",
                    "  candidate:\n    name: Validate exact release identity\n    runs-on: ubuntu-24.04\n    permissions:\n      contents: read\n      id-token: write",
                    1,
                ),
                "must not grant id-token permission to candidate",
            ),
            (
                valid.replacen(CRATES_IO_AUTH_ACTION, "rust-lang/crates-io-auth-action@v1", 1),
                "reviewed SHA-pinned action",
            ),
            (
                valid.replacen(
                    "        uses: rust-lang/crates-io-auth-action@c6f97d42243bad5fab37ca0427f495c86d5b1a18 # v1.0.5",
                    "        uses: rust-lang/crates-io-auth-action@c6f97d42243bad5fab37ca0427f495c86d5b1a18 # v1.0.5\n        with:\n          url: https://staging.crates.io",
                    1,
                ),
                "no inputs or fail-open behavior",
            ),
            (
                valid.replacen(
                    "        id: crates-io-auth\n        if: needs.candidate.outputs.mode == 'publish' && steps.state.outputs.crate-exists != 'true'",
                    "        id: crates-io-auth\n        if: needs.candidate.outputs.mode == 'recover'",
                    1,
                ),
                "exact publish-only condition",
            ),
            (
                valid.replacen(
                    "          CARGO_REGISTRY_TOKEN: ${{ steps.crates-io-auth.outputs.token }}",
                    "          CARGO_REGISTRY_TOKEN: ${{ secrets.CARGO_REGISTRY_TOKEN }}",
                    1,
                ),
                "must not reference a long-lived CARGO_REGISTRY_TOKEN secret",
            ),
            (
                valid.replacen(
                    "          CARGO_REGISTRY_TOKEN: ${{ steps.crates-io-auth.outputs.token }}",
                    "          CARGO_REGISTRY_TOKEN: ${{ steps.untrusted.outputs.token }}",
                    1,
                ),
                "bind only the short-lived crates.io-auth action output",
            ),
            (
                valid.replacen(
                    "        uses: rust-lang/crates-io-auth-action@c6f97d42243bad5fab37ca0427f495c86d5b1a18 # v1.0.5\n\n      - name: Publish exact crates.io package",
                    "        uses: rust-lang/crates-io-auth-action@c6f97d42243bad5fab37ca0427f495c86d5b1a18 # v1.0.5\n\n      - name: Delay credential use\n        run: true\n\n      - name: Publish exact crates.io package",
                    1,
                ),
                "authenticate immediately after immutable registry inspection",
            ),
            (
                valid.replacen("        continue-on-error: true", "        continue-on-error: false", 1),
                "fail-reconciled and unreachable in recovery mode",
            ),
        ];
        for (drifted, expected) in cases {
            assert_ne!(drifted, valid, "mutation fixture did not match: {expected}");
            let errors = publish_errors(&drifted).join("\n");
            assert!(errors.contains(expected), "missing {expected}: {errors}");
        }
    }

    #[test]
    fn release_recovery_contract_rejects_identity_and_mutation_regressions() {
        let valid = workflow_text("release-publish.yml");
        assert_eq!(publish_errors(&valid), Vec::<String>::new());

        let cases = [
            (
                valid.replacen(
                    "type: choice\n        options:\n          - publish\n          - recover",
                    "type: string",
                    1,
                ),
                "publish-or-recover mode choice",
            ),
            (
                valid.replacen(
                    "mode: ${{ steps.identity.outputs.mode || steps.recovery-identity.outputs.mode }}",
                    "mode: ${{ steps.identity.outputs.mode }}",
                    1,
                ),
                "select the exact publish or recovery identity",
            ),
            (
                valid.replacen(
                    "control-revision: ${{ steps.identity.outputs.control-revision || steps.recovery-identity.outputs.control-revision }}",
                    "control-revision: ${{ steps.identity.outputs.control-revision }}",
                    1,
                ),
                "select the exact publish or recovery identity",
            ),
            (
                valid.replacen(
                    "git merge-base --is-ancestor \"$REVISION\" refs/remotes/origin/main",
                    "true",
                    1,
                ),
                "must include git merge-base --is-ancestor",
            ),
            (
                valid.replacen(
                    ".version.num == $version and .version.yanked == false and .version.checksum == $checksum",
                    ".version.num == $version",
                    1,
                ),
                "version.checksum",
            ),
            (
                valid.replacen(
                    "if: needs.candidate.outputs.mode == 'publish' && steps.state.outputs.crate-exists != 'true'",
                    "if: steps.state.outputs.crate-exists != 'true'",
                    1,
                ),
                "unreachable in recovery mode",
            ),
            (
                valid.replacen(
                    "if test \"$MODE\" = recover; then\n            git merge-base --is-ancestor \"$REVISION\" refs/remotes/origin/main\n          else",
                    "if test \"$MODE\" = recover; then\n            true\n          else",
                    1,
                ),
                "must include git merge-base --is-ancestor",
            ),
            (
                valid.replacen(
                    "          [[ \"$CONTROL_REVISION\" =~ ^[0-9a-f]{40}$ ]]\n          test \"$CONTROL_REVISION\" = \"$GITHUB_SHA\"\n          test \"$CONTROL_REVISION\" = \"$(git rev-parse refs/remotes/origin/main)\"",
                    "          true",
                    1,
                ),
                "must include test \"$CONTROL_REVISION\" = \"$GITHUB_SHA\"",
            ),
            (
                valid.replacen(
                    "          git fetch --no-tags origin +refs/heads/main:refs/remotes/origin/main\n          test \"$CONTROL_REVISION\" = \"$GITHUB_SHA\"\n          test \"$CONTROL_REVISION\" = \"$(git rev-parse refs/remotes/origin/main)\"\n          if test \"$MODE\" = recover; then",
                    "          git fetch --no-tags origin +refs/heads/main:refs/remotes/origin/main\n          if test \"$MODE\" = recover; then",
                    1,
                ),
                "must include test \"$CONTROL_REVISION\" = \"$GITHUB_SHA\"",
            ),
            (
                valid.replacen(
                    "            git tag -s -m \"Git Slop ${TAG}\" \"$TAG\" \"$REVISION\"\n            git verify-tag \"$TAG\"",
                    "            git tag -f -s -m \"Git Slop ${TAG}\" \"$TAG\" \"$REVISION\"\n            git verify-tag \"$TAG\"",
                    1,
                ),
                "must not include git tag -f",
            ),
            (
                valid.replacen(
                    "RELEASE_SIGNING_PRIVATE_KEY: ${{ secrets.RELEASE_SIGNING_PRIVATE_KEY }}",
                    "RELEASE_SIGNING_PRIVATE_KEY: ${{ secrets.OTHER_SIGNING_KEY }}",
                    1,
                ),
                "must reference the release signing secret exactly once",
            ),
            (
                valid.replacen(
                    "- name: Create missing exact release tag",
                    "- name: Create unsigned exact release tag",
                    1,
                ),
                "must expose the release signing secret only to the exact tag-creation step",
            ),
        ];
        for (drifted, expected) in cases {
            let errors = publish_errors(&drifted).join("\n");
            assert!(errors.contains(expected), "missing {expected}: {errors}");
        }
    }

    #[test]
    fn relay_and_homebrew_contracts_reject_trigger_asset_and_secret_drift() {
        let relay = workflow_text("release-published.yml");
        assert_eq!(relay_errors(&relay), Vec::<String>::new());
        for (drifted, expected) in [
            (
                relay.replace("types: [published]", "types: [created]"),
                "only for release.published",
            ),
            (
                relay.replacen(
                    "permissions:\n  contents: read",
                    "permissions:\n  actions: write\n  contents: read",
                    1,
                ),
                "must not receive Actions write permission",
            ),
            (
                relay.replacen(
                    "\"git-slop-${TAG}-aarch64-pc-windows-msvc.zip\"",
                    "\"unexpected.zip\"",
                    1,
                ),
                "exact twelve release assets",
            ),
            (
                format!("{relay}\n# gh workflow run homebrew-handoff.yml\n"),
                "must remain verification-only",
            ),
            (
                relay.replacen(
                    "${{ secrets.SCOOP_BUCKET_DISPATCH_TOKEN }}",
                    "${{ github.token }}",
                    1,
                ),
                "reference the Scoop dispatch secret exactly once",
            ),
            (
                relay.replacen("needs: verify-publication", "needs: []", 1),
                "dispatch-scoop needs do not match",
            ),
            (
                relay.replacen(
                    "--field release_manifest_sha256=\"$RELEASE_MANIFEST_SHA256\"",
                    "--field x86_64_sha256=\"$RELEASE_MANIFEST_SHA256\"",
                    1,
                ),
                "--field release_manifest_sha256",
            ),
        ] {
            let errors = relay_errors(&drifted).join("\n");
            assert!(errors.contains(expected), "missing {expected}: {errors}");
        }

        let homebrew = workflow_text("homebrew-handoff.yml");
        assert_eq!(homebrew_errors(&homebrew), Vec::<String>::new());
        for (drifted, expected) in [
            (
                homebrew.replacen("environment: release", "environment: unprotected", 1),
                "required release environment",
            ),
            (
                homebrew.replacen("ref: main", "ref: feature", 1),
                "checkout main without persisted credentials",
            ),
            (
                homebrew.replacen(
                    "${{ secrets.HOMEBREW_TAP_DISPATCH_TOKEN }}",
                    "${{ github.token }}",
                    1,
                ),
                "exactly one step",
            ),
            (
                homebrew.replacen(
                    "\"git-slop-${tag}-aarch64-pc-windows-msvc.zip\"",
                    "\"unexpected.zip\"",
                    1,
                ),
                "exact twelve release assets",
            ),
            (
                homebrew.replacen(
                    "wc -l < release-assets/SHA256SUMS | tr -d ' ')\" = \"11\"",
                    "wc -l < release-assets/SHA256SUMS | tr -d ' ')\" = \"6\"",
                    1,
                ),
                "must include test \"$(wc -l < release-assets/SHA256SUMS",
            ),
            (
                homebrew.replacen(
                    "git merge-base --is-ancestor \"$REVISION\" refs/remotes/origin/main",
                    "git cat-file -e \"$REVISION^{commit}\"",
                    1,
                ),
                "must include git merge-base --is-ancestor",
            ),
            (
                homebrew.replacen(
                    "assert_match \"\\\"source_dirty",
                    "assert_match %(\"source_dirty",
                    1,
                ),
                "must include assert_match \"\\\"source_dirty",
            ),
            (
                homebrew.replacen(
                    "if grep -Eq '^  version[[:space:]]' release-assets/git-slop.rb",
                    "grep -Fx \"  version \\\"${VERSION}\\\"\" release-assets/git-slop.rb",
                    1,
                ),
                "must include if grep -Eq '^  version",
            ),
        ] {
            let errors = homebrew_errors(&drifted).join("\n");
            assert!(errors.contains(expected), "missing {expected}: {errors}");
        }
    }

    #[test]
    fn runtime_launcher_test_must_run_in_rust_quality_job() {
        let valid = r#"jobs:
  rust-quality:
    steps:
      - run: bash scripts/with-agent-plugins.test.sh
"#;
        let mut errors = Vec::new();
        validate_runtime_launcher_ci_job(valid, "ci.yml", &mut errors);
        assert_eq!(errors, Vec::<String>::new());

        let wrong_job = r#"jobs:
  workflow-lint:
    steps:
      - run: bash scripts/with-agent-plugins.test.sh
  rust-quality:
    steps:
      - run: cargo test
"#;
        let mut errors = Vec::new();
        validate_runtime_launcher_ci_job(wrong_job, "ci.yml", &mut errors);
        assert!(
            errors
                .iter()
                .any(|error| error.contains("rust-quality job must run"))
        );

        let expanded_command = r#"jobs:
  rust-quality:
    steps:
      - run: |
          echo preparing
          bash scripts/with-agent-plugins.test.sh
"#;
        let mut errors = Vec::new();
        validate_runtime_launcher_ci_job(expanded_command, "ci.yml", &mut errors);
        assert!(
            errors
                .iter()
                .any(|error| error.contains("rust-quality job must run"))
        );
    }

    fn windows_action_ci_errors(text: &str) -> Vec<String> {
        let mut errors = Vec::new();
        validate_windows_action_ci_job(text, "ci.yml", &mut errors);
        errors
    }

    fn valid_windows_action_ci() -> &'static str {
        r#"jobs:
  platform-smoke:
    strategy:
      matrix:
        os:
          - ubuntu-24.04
          - macos-15
          - windows-2025
          - windows-11-arm
    runs-on: ${{ matrix.os }}
    steps:
      - name: Set up Node.js for Windows Action tests
        if: runner.os == 'Windows'
        uses: actions/setup-node@v7
        with:
          node-version: "24"
      - name: Test GitHub Action on Windows
        if: runner.os == 'Windows'
        run: node --test action/install.test.mjs
"#
    }

    #[test]
    fn windows_action_ci_contract_accepts_node_24_test() {
        assert_eq!(
            windows_action_ci_errors(valid_windows_action_ci()),
            Vec::<String>::new()
        );
    }

    #[test]
    fn windows_action_ci_contract_rejects_node_version_drift() {
        let drifted =
            valid_windows_action_ci().replace("node-version: \"24\"", "node-version: \"22\"");
        let errors = windows_action_ci_errors(&drifted).join("\n");
        assert!(errors.contains("must install Node.js 24"), "{errors}");
    }

    #[test]
    fn windows_action_ci_contract_rejects_condition_drift() {
        let drifted = valid_windows_action_ci().replacen(
            "if: runner.os == 'Windows'",
            "if: runner.os != 'Windows'",
            1,
        );
        let errors = windows_action_ci_errors(&drifted).join("\n");
        assert!(
            errors.contains("must use the exact Windows runner condition"),
            "{errors}"
        );
    }

    #[test]
    fn windows_action_ci_contract_rejects_command_drift() {
        let drifted = valid_windows_action_ci().replace(
            "node --test action/install.test.mjs",
            "node --test action/*.test.mjs",
        );
        let errors = windows_action_ci_errors(&drifted).join("\n");
        assert!(errors.contains("must run exactly"), "{errors}");
    }

    #[test]
    fn windows_action_ci_contract_rejects_missing_windows_x64_lane() {
        let drifted = valid_windows_action_ci().replace("          - windows-2025\n", "");
        let errors = windows_action_ci_errors(&drifted).join("\n");
        assert!(
            errors.contains("exact supported platform matrix"),
            "{errors}"
        );
    }

    #[test]
    fn windows_action_ci_contract_rejects_missing_windows_arm64_lane() {
        let drifted = valid_windows_action_ci().replace("          - windows-11-arm\n", "");
        let errors = windows_action_ci_errors(&drifted).join("\n");
        assert!(
            errors.contains("exact supported platform matrix"),
            "{errors}"
        );
    }

    #[test]
    fn windows_action_ci_contract_rejects_wrong_runs_on() {
        let drifted =
            valid_windows_action_ci().replace("runs-on: ${{ matrix.os }}", "runs-on: windows-2025");
        let errors = windows_action_ci_errors(&drifted).join("\n");
        assert!(errors.contains("must use matrix.os as runs-on"), "{errors}");
    }

    #[test]
    fn windows_action_ci_contract_rejects_excluded_windows_lane() {
        let drifted = valid_windows_action_ci().replace(
            "          - windows-11-arm\n    runs-on:",
            "          - windows-11-arm\n        exclude:\n          - os: windows-11-arm\n    runs-on:",
        );
        let errors = windows_action_ci_errors(&drifted).join("\n");
        assert!(
            errors.contains("must not exclude either supported Windows lane"),
            "{errors}"
        );
    }

    #[test]
    fn windows_action_ci_contract_rejects_missing_setup() {
        let missing_setup = r#"jobs:
  platform-smoke:
    strategy:
      matrix:
        os:
          - ubuntu-24.04
          - macos-15
          - windows-2025
          - windows-11-arm
    runs-on: ${{ matrix.os }}
    steps:
      - name: Test GitHub Action on Windows
        if: runner.os == 'Windows'
        run: node --test action/install.test.mjs
"#;
        let errors = windows_action_ci_errors(missing_setup).join("\n");
        assert!(
            errors.contains("must define exactly one Set up Node.js for Windows Action tests"),
            "{errors}"
        );
    }

    #[test]
    fn windows_action_ci_contract_rejects_reordered_setup() {
        let reordered = r#"jobs:
  platform-smoke:
    strategy:
      matrix:
        os:
          - ubuntu-24.04
          - macos-15
          - windows-2025
          - windows-11-arm
    runs-on: ${{ matrix.os }}
    steps:
      - name: Test GitHub Action on Windows
        if: runner.os == 'Windows'
        run: node --test action/install.test.mjs
      - name: Set up Node.js for Windows Action tests
        if: runner.os == 'Windows'
        uses: actions/setup-node@v7
        with:
          node-version: "24"
"#;
        let errors = windows_action_ci_errors(reordered).join("\n");
        assert!(
            errors.contains("must run before Test GitHub Action on Windows"),
            "{errors}"
        );
    }

    #[test]
    fn execution_state_artifacts_require_early_root_and_guarded_upload() {
        let valid = r#"jobs:
  sync:
    steps:
      - name: Prepare artifact root
      - name: Prepare pinned agent-plugins runtime
      - name: Upload execution artifacts
        if: ${{ (failure() || github.event_name == 'workflow_dispatch') && steps.artifact-root.outputs.path != '' }}
        with:
          path: ${{ steps.artifact-root.outputs.path }}
          include-hidden-files: true
          if-no-files-found: error
          retention-days: 14
"#;
        let mut errors = Vec::new();
        validate_execution_state_artifacts(valid, &mut errors);
        assert_eq!(errors, Vec::<String>::new());

        let late_and_unguarded = valid
            .replace(
                "      - name: Prepare artifact root\n      - name: Prepare pinned agent-plugins runtime",
                "      - name: Prepare pinned agent-plugins runtime\n      - name: Prepare artifact root",
            )
            .replace(
                "if: ${{ (failure() || github.event_name == 'workflow_dispatch') && steps.artifact-root.outputs.path != '' }}",
                "if: ${{ failure() }}",
            );
        let mut errors = Vec::new();
        validate_execution_state_artifacts(&late_and_unguarded, &mut errors);
        assert!(
            errors
                .iter()
                .any(|error| error.contains("before private runtime preparation"))
        );
        assert!(
            errors
                .iter()
                .any(|error| error.contains("artifact-root.outputs.path != ''"))
        );
    }
}
