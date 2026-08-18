    #[test]
    fn repository_workflows_pass() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
        assert_eq!(validate(root), Vec::<String>::new());
    }

    include!("repository_contracts.rs");

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
                valid.replacen("        timeout-minutes: 5", "        timeout-minutes: 30", 1),
                "cap musl package setup at five minutes",
            ),
            (
                valid.replacen(
                    "https://archive.ubuntu.com/ubuntu/",
                    "http://azure.archive.ubuntu.com/ubuntu/",
                    1,
                ),
                "must not include azure.archive.ubuntu.com",
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
                    "Homebrew/actions/setup-homebrew@8f3d1ec8a696b3b9d9a6c3696b6c73033cab69e4",
                    "actions/setup-homebrew@8f3d1ec8a696b3b9d9a6c3696b6c73033cab69e4",
                    1,
                ),
                "must use the Homebrew setup Action",
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
