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
const RELEASE_TARGETS: [&str; 5] = [
    "aarch64-apple-darwin",
    "aarch64-pc-windows-msvc",
    "aarch64-unknown-linux-gnu",
    "x86_64-pc-windows-msvc",
    "x86_64-unknown-linux-gnu",
];
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
            "cargo xtask release-manifest",
            "--crate-source candidate/crate-source.json",
            "cargo xtask homebrew-formula",
            "sha256sum git-slop.rb >> SHA256SUMS",
            "wc -l < candidate-dist/SHA256SUMS",
            "= \"7\"",
        ] {
            require(run, required, name, errors);
        }
        forbid(
            run,
            "sha256sum git-slop.rb release-manifest.json",
            name,
            errors,
        );
    }

    if let Some(publish_crate) = publish_crate {
        require_needs(
            publish_crate,
            name,
            "publish-crate",
            &["candidate", "candidate-distribution"],
            errors,
        );
        require_environment(publish_crate, name, "publish-crate", "release", errors);
        validate_publish_token_scope(payload, errors);
        validate_publish_order_and_registry(publish_crate, errors);
        let Some(revalidate) = step_run(
            publish_crate,
            "Revalidate protected release identity after environment approval",
        ) else {
            errors.push(format!(
                "{name} publish-crate must revalidate the protected release identity after environment approval."
            ));
            return;
        };
        if named_step(
            publish_crate,
            "Revalidate protected release identity after environment approval",
        )
        .and_then(|step| step_env(step, "CONTROL_REVISION"))
            != Some("${{ needs.candidate.outputs.control-revision }}")
        {
            errors.push(format!(
                "{name} protected release revalidation must bind the exact trusted workflow control revision."
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
        let Some(generate) = step_run(draft, "Generate release manifest, checksums, and Formula")
        else {
            errors.push(format!(
                "{name} draft-release must generate final metadata."
            ));
            return;
        };
        require(
            generate,
            "sha256sum git-slop.rb >> SHA256SUMS",
            name,
            errors,
        );
        require(
            generate,
            "test \"$(wc -l < dist/SHA256SUMS | tr -d ' ')\" = \"7\"",
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
            "gh release create \"$TAG\" --draft",
            "gh release upload \"$TAG\"",
            "GIT_SLOP_ALLOW_DRAFT_RELEASE: \"true\"",
        ] {
            require(text, required, name, errors);
        }
        for forbidden in [
            "gh release edit",
            "--draft=false",
            "-f draft=false",
            "-F draft=false",
        ] {
            forbid(text, forbidden, name, errors);
        }
        if let Some(verify) = step_run(draft, "Verify published no-op or refreshed draft assets") {
            validate_exact_release_assets(verify, name, "${TAG}", errors);
        } else {
            errors.push(format!(
                "{name} must verify the exact eight release assets."
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
                Some(run) => require(run, "node action/install.mjs", name, errors),
                None => errors.push(format!(
                    "{name} draft installer verification must run action/install.mjs."
                )),
            }
        } else {
            errors.push(format!(
                "{name} draft-release must verify the Action installer against release assets."
            ));
        }
        if let Some(assert_outputs) = named_step(draft, "Assert exact Action installer outputs") {
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
        require_needs(
            draft_action_smoke,
            name,
            "draft-action-smoke",
            &["publish-crate", "draft-release"],
            errors,
        );
        validate_target_matrix(
            draft_action_smoke,
            name,
            "draft-action-smoke",
            false,
            errors,
        );
        for required in [
            "Install and verify the draft release through the public Action installer",
            "GIT_SLOP_ALLOW_DRAFT_RELEASE: \"true\"",
            "node action/install.mjs",
        ] {
            require(text, required, name, errors);
        }
    }

    if let Some(marketplace_ready) = marketplace_ready {
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
        for required in [
            "Marketplace-ready",
            "Open the draft release",
            "published-release relay",
            "existing published release was reverified without mutation",
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

fn validate_publish_token_scope(payload: &YamlValue, errors: &mut Vec<String>) {
    let name = "release-publish.yml";
    let token = "${{ secrets.CARGO_REGISTRY_TOKEN }}";
    if workflow_or_job_env_contains(payload, "CARGO_REGISTRY_TOKEN") {
        errors.push(format!(
            "{name} must not expose CARGO_REGISTRY_TOKEN at workflow or job scope."
        ));
    }
    if yaml_scalar_occurrences(payload, token) != 1 {
        errors.push(format!(
            "{name} must reference the Cargo registry secret exactly once."
        ));
    }
    let token_steps = workflow_steps(payload)
        .into_iter()
        .filter(|step| env_has_key(step, "CARGO_REGISTRY_TOKEN"))
        .collect::<Vec<_>>();
    if token_steps.len() != 1 {
        errors.push(format!(
            "{name} must bind CARGO_REGISTRY_TOKEN to exactly one step."
        ));
        return;
    }
    let step = token_steps[0];
    if step.get("name").and_then(YamlValue::as_str) != Some("Publish first crates.io package") {
        errors.push(format!(
            "{name} must expose CARGO_REGISTRY_TOKEN only to Publish first crates.io package."
        ));
    }
    let token = step
        .get("env")
        .and_then(|env| env.get("CARGO_REGISTRY_TOKEN"))
        .and_then(YamlValue::as_str);
    if token != Some("${{ secrets.CARGO_REGISTRY_TOKEN }}") {
        errors.push(format!(
            "{name} publish step must source CARGO_REGISTRY_TOKEN from the release secret."
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
    if step.get("if").and_then(YamlValue::as_str)
        != Some(
            "needs.candidate.outputs.mode == 'publish' && steps.state.outputs.crate-exists != 'true'",
        )
    {
        errors.push(format!(
            "{name} credentialed publish step must be unreachable in recovery mode."
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
        .position(|step| *step == "Publish first crates.io package");
    let registry = names
        .iter()
        .position(|step| *step == "Download and verify exact registry bytes");
    let tag = names
        .iter()
        .position(|step| *step == "Create missing exact release tag");
    if !matches!((publish, registry, tag), (Some(publish), Some(registry), Some(tag)) if publish < registry && registry < tag)
    {
        errors.push(format!(
            "{name} must publish, verify registry bytes, and only then create the release tag."
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
    require_permission(payload, name, "actions", "write", errors);
    require_permission(payload, name, "contents", "read", errors);
    if text.contains("secrets.") {
        errors.push(format!("{name} must not consume named secrets."));
    }
    let Some(jobs) = payload.get("jobs").and_then(YamlValue::as_mapping) else {
        errors.push(format!("{name} must define jobs."));
        return;
    };
    require_exact_job_set(jobs, name, &["verify-and-relay"], errors);
    let Some(relay) = job(jobs, "verify-and-relay", name, errors) else {
        return;
    };
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
    ] {
        require(verify, required, name, errors);
    }
    validate_exact_release_assets(verify, name, "${TAG}", errors);
    let Some(dispatch) = step_run(relay, "Dispatch protected Homebrew handoff from main") else {
        errors.push(format!(
            "{name} must dispatch the protected Homebrew handoff."
        ));
        return;
    };
    for required in [
        "gh workflow run homebrew-handoff.yml",
        "--repo \"$REPOSITORY\"",
        "--ref main",
        "--field version=\"$VERSION\"",
        "--field revision=\"$REVISION\"",
    ] {
        require(dispatch, required, name, errors);
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
    let Some(main_check) = step_run(
        handoff,
        "Revalidate trusted main after environment approval",
    ) else {
        errors.push(format!(
            "{name} must revalidate trusted main after approval."
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
        "test \"$(wc -l < release-assets/SHA256SUMS | tr -d ' ')\" = \"7\"",
        "sha256sum --check SHA256SUMS",
        ".crate_source.registry == \"crates.io\"",
        "https://static.crates.io/crates/git-slop/git-slop-",
        "(.artifacts | length) == 5",
        "git merge-base --is-ancestor \"$REVISION\" refs/remotes/origin/main",
        "curl --fail --location --retry 5 \"$crate_url\"",
        "test \"$(sha256sum registry.crate | awk '{print $1}')\" = \"$crate_sha256\"",
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
    if yaml_scalar_occurrences(payload, token) != 1 {
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
        "--field formula_url=\"$FORMULA_URL\"",
        "--field formula_sha256=\"$FORMULA_SHA256\"",
        "--field manifest_url=\"$MANIFEST_URL\"",
        "--field manifest_sha256=\"$MANIFEST_SHA256\"",
        "--field crate_url=\"$CRATE_URL\"",
        "--field crate_sha256=\"$CRATE_SHA256\"",
    ] {
        require(run, required, name, errors);
    }
}

fn validate_exact_release_assets(run: &str, name: &str, tag: &str, errors: &mut Vec<String>) {
    let normalized = run
        .split_whitespace()
        .filter(|token| *token != "\\")
        .collect::<Vec<_>>()
        .join(" ");
    let exact = format!(
        "printf '%s\\n' SHA256SUMS git-slop.rb \
         \"git-slop-{tag}-aarch64-apple-darwin.tar.gz\" \
         \"git-slop-{tag}-aarch64-pc-windows-msvc.zip\" \
         \"git-slop-{tag}-aarch64-unknown-linux-gnu.tar.gz\" \
         \"git-slop-{tag}-x86_64-pc-windows-msvc.zip\" \
         \"git-slop-{tag}-x86_64-unknown-linux-gnu.tar.gz\" \
         release-manifest.json | LC_ALL=C sort > expected-assets.txt"
    );
    if !normalized.contains(&exact) {
        errors.push(format!(
            "{name} must compare the exact eight release assets against the published inventory."
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

fn yaml_scalar_occurrences(value: &YamlValue, expected: &str) -> usize {
    match value {
        YamlValue::String(value) => usize::from(value == expected),
        YamlValue::Sequence(values) => values
            .iter()
            .map(|value| yaml_scalar_occurrences(value, expected))
            .sum(),
        YamlValue::Mapping(values) => values
            .values()
            .map(|value| yaml_scalar_occurrences(value, expected))
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
            "{name} {job_name} must use the protected {expected} environment."
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
            "{name} {job_name} must contain exactly the five supported targets."
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
                    CRATES_IO_RELEASE_USER_AGENT,
                    r#"--user-agent "curl/8""#,
                    1,
                ),
                "identify every crates.io API request",
            ),
            (
                valid.replacen("environment: release", "environment: unprotected", 1),
                "protected release environment",
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
                "only then create the release tag",
            ),
            (
                valid.replacen(
                    "target: aarch64-pc-windows-msvc",
                    "target: unsupported-target",
                    1,
                ),
                "exactly the five supported targets",
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
        ];
        for (drifted, expected) in cases {
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
                    "            git tag \"$TAG\" \"$REVISION\"\n          fi\n          authorization=",
                    "            git tag -f \"$TAG\" \"$REVISION\"\n          fi\n          authorization=",
                    1,
                ),
                "must not include git tag -f",
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
                relay.replacen("--ref main", "--ref feature", 1),
                "must include --ref main",
            ),
            (
                relay.replacen(
                    "\"git-slop-${TAG}-aarch64-pc-windows-msvc.zip\"",
                    "\"unexpected.zip\"",
                    1,
                ),
                "exact eight release assets",
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
                "protected release environment",
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
                "exact eight release assets",
            ),
            (
                homebrew.replacen(
                    "wc -l < release-assets/SHA256SUMS | tr -d ' ')\" = \"7\"",
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
