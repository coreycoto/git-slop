fn validate_candidate_job(candidate: Option<&YamlValue>, errors: &mut Vec<String>) {
    let name = "release-publish.yml";
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

}
