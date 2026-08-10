fn validate_publish_crate_job(publish_crate: Option<&YamlValue>, text: &str, payload: &YamlValue, errors: &mut Vec<String>) {
    let name = "release-publish.yml";
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

}
