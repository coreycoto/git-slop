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
                    ".artifacts[].name, .supplemental_assets[].name",
                    ".artifacts[].name",
                    1,
                ),
                "derive the required release-asset inventory",
            ),
            (
                relay.replacen(".immutable == true", ".immutable == false", 1),
                ".immutable == true",
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
            (
                relay.replacen("--repo \"$GITHUB_REPOSITORY\"", "", 1),
                "--repo \"$GITHUB_REPOSITORY\"",
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
                    ".artifacts[].name, .supplemental_assets[].name",
                    ".artifacts[].name",
                    1,
                ),
                "derive the required release-asset inventory",
            ),
            (
                homebrew.replacen(".immutable == true", ".immutable == false", 1),
                ".immutable == true",
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
