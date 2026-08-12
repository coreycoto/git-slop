fn validate_draft_release_job(draft: Option<&YamlValue>, text: &str, errors: &mut Vec<String>) {
    let name = "release-publish.yml";
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
                    != Some("action\nxtask\n")
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
        require(
            generate,
            "xtask=(cargo run --quiet --manifest-path release-control/xtask/Cargo.toml -- --repo-root .)",
            name,
            errors,
        );
        require(
            generate,
            r#""${xtask[@]}" sbom --output-dir dist"#,
            name,
            errors,
        );
        require(
            generate,
            "(cd dist && sha256sum --check SHA256SUMS)",
            name,
            errors,
        );
        if generate
            .matches(r#""${xtask[@]}" release-manifest"#)
            .count()
            != 2
        {
            errors.push(format!(
                "{name} final distribution must regenerate the manifest after Formula and SBOM generation."
            ));
        }
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
            "gh release create \"$TAG\" --draft --notes-file release-notes.md --title \"$TAG\" --target \"$REVISION\" --verify-tag",
            "GIT_SLOP_ALLOW_DRAFT_RELEASE: \"true\"",
            "GIT_SLOP_RELEASE_ID:",
        ] {
            require(text, required, name, errors);
        }
        if let Some(notes) = step_run(draft, "Generate complete release notes") {
            for required in [
                "CHANGELOG.md release heading for ${VERSION} is still Unreleased.",
                "cargo install git-slop --version %s --locked",
                "brew install coreycoto/tap/git-slop",
                "Windows bucket",
                "sha256sum --check SHA256SUMS --ignore-missing",
                "gh attestation verify git-slop-v%s-%s",
                "git-slop.cdx.json",
                "git-slop.spdx.json",
                "Live verification surfaces",
            ] {
                require(notes, required, name, errors);
            }
        } else {
            errors.push(format!(
                "{name} must generate complete versioned release notes before draft creation."
            ));
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
                ".target_commitish == $revision",
                "(.tag_name | startswith(\"untagged-\"))",
                "match_count=\"$(jq -r 'length' release-matches.json)\"",
                "case \"$match_count\" in",
                "release_id=\"$(jq -er '.id | select(type == \"number\" and . > 0)' release.json)\"",
                ".draft == false and .immutable == true",
                "Multiple GitHub Releases match exact or safely detached identity ${TAG}; refusing ambiguous release mutation.",
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
                    "gh release create \"$TAG\" --draft --notes-file release-notes.md --title \"$TAG\" --target \"$REVISION\" --verify-tag",
                    "for attempt in $(seq 1 10); do",
                    "gh api --paginate --slurp \"repos/${GITHUB_REPOSITORY}/releases?per_page=100\"",
                    ".target_commitish == $revision",
                    "(.tag_name | startswith(\"untagged-\"))",
                    "match_count=\"$(jq -r 'length' release-matches.json)\"",
                    "if test \"$match_count\" -gt 1; then",
                    "release_id=\"$(jq -er '.[0].id | select(type == \"number\" and . > 0)' release-matches.json)\"",
                    "Multiple GitHub Releases match exact or safely detached identity ${TAG}; refusing ambiguous release mutation.",
                    "sleep 2",
                    "repos/${GITHUB_REPOSITORY}/releases/${release_id}",
                    "{tag_name: $tag, target_commitish: $revision, name: $tag, body: $body, draft: true, prerelease: false}",
                    "gh api --method PATCH \"$endpoint\" --input release-update.json",
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
                if run
                    .matches("gh api --method PATCH \"$endpoint\" --input release-update.json")
                    .count()
                    != 2
                {
                    errors.push(format!(
                        "{name} draft refresh must reassert the exact signed tag after all asset uploads."
                    ));
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
                "[[ \"$RELEASE_ID\" =~ ^[1-9][0-9]*$ ]]",
                "gh api \"repos/${GITHUB_REPOSITORY}/releases/${RELEASE_ID}\" > release.json",
                "repos/${GITHUB_REPOSITORY}/releases/assets/${asset_id}",
                ".id == $release_id",
                ".draft == false and .immutable == true",
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
                "{name} must verify the manifest-derived release assets."
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

}
