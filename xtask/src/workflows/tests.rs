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
    fn every_external_action_surface_requires_a_full_commit_sha() {
        let root = tempfile::tempdir().unwrap();
        let workflows = root.path().join(".github/workflows");
        fs::create_dir_all(&workflows).unwrap();
        fs::write(
            root.path().join("action.yml"),
            "runs:\n  using: composite\n  steps:\n    - uses: actions/cache@0123456789abcdef0123456789abcdef01234567\n",
        )
        .unwrap();
        fs::write(
            workflows.join("unsafe.yml"),
            "jobs:\n  unsafe:\n    uses: owner/reusable@v1\n",
        )
        .unwrap();
        let mut errors = Vec::new();
        validate_action_versions(root.path(), &workflows, &mut errors);
        assert_eq!(errors.len(), 1, "{errors:?}");
        assert!(errors[0].contains("owner/reusable@v1"));
    }

    #[test]
    fn packaged_contract_validation_requires_a_clean_fixture() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
        let valid = fs::read_to_string(root.join("scripts/validate-packaged-contracts.sh")).unwrap();
        let invalid = valid.replacen(
            "git clone --quiet --no-hardlinks --no-tags \"$source_worktree\" \"$worktree\"",
            "cp -R \"$source_worktree\" \"$worktree\"",
            1,
        );
        let mut errors = Vec::new();
        validate_packaged_contracts_text(&invalid, &mut errors);
        assert!(errors.iter().any(|error| error.contains("git clone")));
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
                    "candidate_source_dir=\"${RUNNER_TEMP}/candidate-source\"",
                    "candidate_source_dir=\"candidate-source\"",
                    1,
                ),
                "unpack candidate source outside the repository workspace",
            ),
            (
                valid.replacen(
                    "$candidateSourceDir = Join-Path $env:RUNNER_TEMP \"candidate-source\"",
                    "$candidateSourceDir = \"candidate-source\"",
                    1,
                ),
                "unpack candidate source outside the repository workspace",
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
                    &format!(r#"-c user.email="{RELEASE_VALIDATION_EMAIL}""#),
                    r#"-c user.email="""#,
                    1,
                ),
                RELEASE_VALIDATION_EMAIL,
            ),
            (
                valid.replacen(
                    "cargo xtask sbom --output-dir candidate-dist",
                    "true # omitted candidate SBOM generation",
                    1,
                ),
                "must include cargo xtask sbom --output-dir candidate-dist",
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
                    "gh release create \"$TAG\" --draft --notes-file release-notes.md --title \"$TAG\" --target \"$REVISION\" --verify-tag",
                    "gh release create \"$TAG\" --draft --notes-file release-notes.md --title \"$TAG\" --target \"$REVISION\"",
                    1,
                ),
                "--verify-tag",
            ),
            (
                valid.replacen(
                    "Multiple GitHub Releases match exact or safely detached identity ${TAG}; refusing ambiguous release mutation.",
                    "Ignoring duplicate or detached releases.",
                    1,
                ),
                "Multiple GitHub Releases match exact or safely detached identity",
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
                    "gh api \"repos/${GITHUB_REPOSITORY}/releases/${RELEASE_ID}\" > release.json",
                    "true",
                    1,
                ),
                "releases/${RELEASE_ID}",
            ),
            (
                valid.replacen(
                    "gh api --method PATCH \"$endpoint\" --input release-update.json > release.json",
                    "gh api \"$endpoint\" > release.json",
                    1,
                ),
                "reassert the exact signed tag after all asset uploads",
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
                relay.replacen(
                    "gh workflow run dependency-remediation.yml \\\n            --repo \"$GITHUB_REPOSITORY\"",
                    "gh workflow run dependency-remediation.yml \\\n            --repo coreycoto/wrong-repository",
                    1,
                ),
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
        uses: actions/setup-node@820762786026740c76f36085b0efc47a31fe5020
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

    #[test]
    fn release_publish_workflow_is_exactly_generated_from_stage_fragments() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
        let rendered = render_release_workflow(root).expect("render release workflow");
        assert_eq!(rendered, workflow_text("release-publish.yml"));
        generate_release_workflow(root, true).expect("generated workflow is current");
    }
}
