fn validate_guidance(repo_root: &Path, errors: &mut Vec<String>) {
    for relative in REQUIRED_GUIDANCE {
        let Some(text) = read_text(repo_root, relative, errors) else {
            continue;
        };
        if !text.contains(EXPECTED_PLUGIN_URL) && !text.contains("agent-plugins") {
            errors.push(format!(
                "{relative} must point readers to the agent-plugins source of truth."
            ));
        }
        if matches!(
            relative,
            "AGENTS.md" | ".agents/README.md" | ".codex/README.md"
        ) {
            if !text.contains("marketplace-source.json") {
                errors.push(format!(
                    "{relative} must mention .agents/plugins/marketplace-source.json."
                ));
            }
            if !text.contains(GIT_SLOP_PLUGIN_DOC_NAME) {
                errors.push(format!(
                    "{relative} must mention {GIT_SLOP_PLUGIN_DOC_NAME}."
                ));
            }
        }
        for forbidden in REMOVED_LOCAL_PLUGIN_REFERENCES {
            if text.contains(forbidden) {
                errors.push(format!("{relative} must not reference {forbidden}."));
            }
        }
    }
}

fn validate_release_workflow(repo_root: &Path, errors: &mut Vec<String>) {
    let contracts: [(&str, &[&str]); 3] = [
        (
            ".github/workflows/release-publish.yml",
            &[
                "workflow_dispatch:",
                "Explicitly authorize publishing exact current main",
                "cargo publish -p git-slop --locked --no-verify",
                "cargo xtask verify-crate",
                "verified-registry-crate",
                "gh release create \"$TAG\" --draft --notes-file release-notes.md --title \"$TAG\" --target \"$REVISION\" --verify-tag",
                "marketplace-ready:",
                "only manual approval for the release",
                "Dispatch immutable release identity to Homebrew tap",
                "secrets.HOMEBREW_TAP_DISPATCH_TOKEN",
            ],
        ),
        (
            ".github/workflows/release-published.yml",
            &[
                "types: [published]",
                "release-manifest.json",
                "Summarize publication verification",
                "without any Actions environment approval",
                "Dispatch immutable release identity to Scoop bucket",
                "secrets.SCOOP_BUCKET_DISPATCH_TOKEN",
                "--repo coreycoto/scoop-bucket",
            ],
        ),
        (
            ".github/workflows/homebrew-handoff.yml",
            &[
                "workflow_dispatch:",
                "environment: release",
                "secrets.HOMEBREW_TAP_DISPATCH_TOKEN",
                "https://static.crates.io/crates/git-slop/",
                "--repo coreycoto/homebrew-tap",
                "--ref main",
            ],
        ),
    ];
    for (relative, required) in contracts {
        let Some(text) = read_text(repo_root, relative, errors) else {
            continue;
        };
        let label = relative.trim_start_matches(".github/workflows/");
        for forbidden in [
            "AGENT_PLUGINS_READ_TOKEN",
            "AGENT_PLUGINS_GIT_TOKEN",
            runtime_manifest::AGENT_PLUGIN_WRAPPER,
            runtime_manifest::MARKETPLACE_SOURCE_MANIFEST,
            runtime_manifest::EXPECTED_RUNTIME_ARCHIVE,
            runtime_manifest::EXPECTED_RUNTIME_REPOSITORY,
            runtime_manifest::EXPECTED_MARKETPLACE_NAME,
            "agent-plugins-runtime",
            "coreycoto/agent-plugins",
        ] {
            if text.contains(forbidden) {
                errors.push(format!(
                    "{label} must keep public release publication decoupled from private \
                     agent-plugins runtime surface {forbidden}."
                ));
            }
        }
        for required in required {
            if !text.contains(required) {
                errors.push(format!("{label} must include {required}."));
            }
        }
        for removed in [
            "scripts/build_release_manifest.py",
            "scripts/release_prepare.py",
            "scripts/update_homebrew_formula.py",
            "scripts/validate_codex_surface.py",
        ] {
            if text.contains(removed) {
                errors.push(format!(
                    "{label} must not reference retired helper {removed}."
                ));
            }
        }
    }
}
