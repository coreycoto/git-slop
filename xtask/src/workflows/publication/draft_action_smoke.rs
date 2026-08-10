fn validate_draft_action_smoke_job(draft_action_smoke: Option<&YamlValue>, payload: &YamlValue, errors: &mut Vec<String>) {
    let name = "release-publish.yml";
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

}
