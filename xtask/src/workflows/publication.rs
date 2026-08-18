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

    validate_candidate_job(candidate, errors);
    validate_candidate_targets_job(candidate_targets, text, errors);
    validate_candidate_distribution_job(candidate_distribution, errors);
    validate_candidate_homebrew_job(candidate_homebrew_audit, errors);
    validate_publish_crate_job(publish_crate, text, payload, errors);
    validate_build_job(build, text, errors);
    validate_draft_release_job(draft, text, errors);
    validate_draft_action_smoke_job(draft_action_smoke, payload, errors);
    validate_marketplace_ready_job(marketplace_ready, errors);
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

fn validate_bounded_musl_install(
    job: &YamlValue,
    step_name: &str,
    job_name: &str,
    errors: &mut Vec<String>,
) {
    let name = "release-publish.yml";
    let Some(step) = named_step(job, step_name) else {
        errors.push(format!("{name} {job_name} must define {step_name}."));
        return;
    };
    if step.get("timeout-minutes").and_then(YamlValue::as_u64) != Some(5) {
        errors.push(format!(
            "{name} {job_name} must cap musl package setup at five minutes."
        ));
    }
    let Some(run) = step.get("run").and_then(YamlValue::as_str) else {
        errors.push(format!("{name} {job_name} {step_name} must define a script."));
        return;
    };
    for required in [
        "https://archive.ubuntu.com/ubuntu/",
        "Acquire::Retries=2",
        "Acquire::http::Timeout=20",
        "Acquire::https::Timeout=20",
        "sudo apt-get \"${apt_network_options[@]}\" update",
        "sudo apt-get \"${apt_network_options[@]}\" install --yes musl-tools",
    ] {
        require(run, required, name, errors);
    }
    forbid(run, "azure.archive.ubuntu.com", name, errors);
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
    let Some(run) = secret_steps[0].get("run").and_then(YamlValue::as_str) else {
        errors.push(format!(
            "{name} exact tag-creation step must configure the verified release signing email."
        ));
        return;
    };
    for required in [
        r#"$1 == "fpr" {print $10; exit}"#,
        r#"gpg --batch --with-colons --list-secret-keys "$signing_key""#,
        r#"match($10, /<[^<>[:space:]]+@[^<>[:space:]]+>/)"#,
        r#"test -n "$signing_email""#,
        r#"git config user.email "$signing_email""#,
    ] {
        require(run, required, name, errors);
    }
    forbid(run, "actions@users.noreply.github.com", name, errors);
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

include!("publication/candidate.rs");
include!("publication/candidate_targets.rs");
include!("publication/candidate_distribution.rs");
include!("publication/candidate_homebrew.rs");
include!("publication/publish_crate.rs");
include!("publication/build.rs");
include!("publication/draft_release.rs");
include!("publication/draft_action_smoke.rs");
include!("publication/marketplace_ready.rs");
