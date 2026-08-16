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
                    "git config user.email \"$signing_email\"",
                    "git config user.email \"actions@users.noreply.github.com\"",
                    1,
                ),
                "git config user.email \"$signing_email\"",
            ),
            (
                valid.replacen(
                    r#"$1 == "fpr" {print $10; exit}"#,
                    r#"$1 == \"fpr\" {print $10; exit}"#,
                    1,
                ),
                r#"$1 == "fpr" {print $10; exit}"#,
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
