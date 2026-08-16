fn validate_product_documentation(repo_root: &Path, errors: &mut Vec<String>) {
    if let Some(text) = read_text(repo_root, "plugins/git-slop/README.md", errors) {
        for client in GIT_SLOP_PLUGIN_CLIENTS {
            if !text.contains(client) {
                errors.push(format!(
                    "plugins/git-slop/README.md must document Agent Plugins client {client}."
                ));
            }
        }
    }
    validate_normalized_contract(
        repo_root,
        "plugins/git-slop/README.md",
        &[
            "https://agent-plugins.org/specification",
            "extensions.com.openai",
            "codex plugin marketplace --help",
            "codex plugin marketplace add coreycoto/git-slop --ref <release>",
            "codex plugin add git-slop@git-slop-marketplace",
            "temporary Codex 0.146.x compatibility overlay",
            "agents/vscode.yaml",
            "agents/cursor.yaml",
            "agents/github.yaml",
            "agents/kiro.yaml",
        ],
        &["Codex CLI 0.146.0 or newer"],
        errors,
    );
    validate_normalized_contract(
        repo_root,
        "plugins/git-slop/skills/run-report/SKILL.md",
        &[
            "git-slop health --report <report.json>",
            "git-slop health --format json",
            "writes its selected rendering to stdout",
            "does not rewrite `health.md`",
            "Use `check`",
            "references/health.md",
        ],
        &[],
        errors,
    );
    validate_normalized_contract(
        repo_root,
        "plugins/git-slop/skills/run-report/references/health.md",
        &[
            "Every format writes to stdout",
            "does not rewrite that file",
            "Exit `0` means the selected report rendered successfully",
            "run `git-slop find` exactly once",
            "git-slop health --report path/to/report.json --format json",
            "git-slop health --report",
            "git-slop check --report",
            "does not modify report artifacts",
        ],
        &[],
        errors,
    );
    validate_normalized_contract(
        repo_root,
        "plugins/git-slop/skills/adopt-repo/SKILL.md",
        &["actions/checkout@v7", "run `find` once"],
        &["actions/checkout@v6"],
        errors,
    );
    validate_normalized_contract(
        repo_root,
        "plugins/git-slop/skills/review-results/SKILL.md",
        &[
            "Treat health output as advisory",
            "references/maintenance-planning.md",
            "explicitly asks for a maintenance proposal",
        ],
        &[],
        errors,
    );
    validate_normalized_contract(
        repo_root,
        "plugins/git-slop/skills/review-results/references/maintenance-planning.md",
        &[
            "git-slop plan --format json",
            "preview-only `backlog_handoff`",
            "Do not create, update, close, label, or milestone GitHub issues",
        ],
        &[],
        errors,
    );
    validate_normalized_contract(
        repo_root,
        "docs/commands.md",
        &[
            "Every format writes to standard output",
            "never rewrites `.slop/latest/health.md`",
            "successful rendering exits 0",
            "Use `git-slop check`",
            "# Repository Health",
            "git-slop explain --path src/parser.rs",
        ],
        &[],
        errors,
    );
    validate_normalized_contract(
        repo_root,
        "docs/report-contract.md",
        &[
            "All three `health` formats write to standard output",
            "do not rewrite `.slop/latest/health.md`",
            "health.data_context_min_bytes",
            "health.folder_bands.refactor_required_max_direct_files",
            "health.summary_top_folders",
        ],
        &[],
        errors,
    );
    validate_normalized_contract(
        repo_root,
        "docs/github-action.md",
        &[
            "Run `git-slop find` once",
            "git-slop health --report",
            "`health` render exits 0",
        ],
        &[],
        errors,
    );
    validate_product_documentation_additions(repo_root, errors);
}
