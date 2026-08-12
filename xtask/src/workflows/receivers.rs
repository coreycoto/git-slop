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
        &["verify-publication", "dispatch-scoop", "exercise-remediation-canary"],
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
    canary::validate_release_canary(jobs, name, errors);
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
        ".tag_name == $tag and .draft == false and .prerelease == false and .immutable == true",
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
        ".tag_name == $tag and .draft == false and .prerelease == false and .immutable == true",
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
