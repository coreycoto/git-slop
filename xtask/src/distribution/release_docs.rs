fn validate_release_documentation(repo_root: &Path, errors: &mut Vec<String>) {
    let read = |relative: &str, errors: &mut Vec<String>| {
        fs::read_to_string(repo_root.join(relative)).unwrap_or_else(|error| {
            errors.push(format!("Unable to read {relative}: {error}"));
            String::new()
        })
    };

    let prompt_relative = ".github/codex/prompts/release-publish.md";
    let prompt = read(prompt_relative, errors);
    require(
        &prompt,
        "seven-target Action smoke matrix",
        prompt_relative,
        errors,
    );
    if prompt.contains("five-target") {
        errors.push(format!(
            "{prompt_relative} must not describe the seven-target Action matrix as five-target."
        ));
    }
    for release_target in RELEASE_TARGETS {
        require(&prompt, release_target.target, prompt_relative, errors);
    }

    let install_relative = "docs/install.md";
    let install = read(install_relative, errors);
    for release_target in RELEASE_TARGETS {
        require(&install, release_target.target, install_relative, errors);
    }

    let archive_relative = "docs/archive-install.md";
    let archive = read(archive_relative, errors);
    for marker in [
        "--pattern release-manifest.json",
        ".artifacts | length == 7",
        "manifest_sha256",
        "actual_size",
        "$HOME/.local/bin",
        "$HOME/.zfunc",
        "$HOME/.config/fish/completions",
    ] {
        require(&archive, marker, archive_relative, errors);
    }
    let windows_relative = "docs/archive-install-windows.md";
    let windows = read(windows_relative, errors);
    require(
        &windows,
        "installed build identity mismatch",
        windows_relative,
        errors,
    );

    let action_docs_relative = "docs/github-action.md";
    let action_docs = read(action_docs_relative, errors);
    if action_docs.contains("removed in v0.14.0") {
        errors.push(format!(
            "{action_docs_relative} must not claim that compatibility aliases present in action.yml were removed in v0.14.0."
        ));
    }
    require(
        &action_docs,
        "future breaking release no earlier than 2026-11-01",
        action_docs_relative,
        errors,
    );

    for relative in ["plugins/git-slop/README.md", "plugins/git-slop/CLIENTS.md"] {
        let text = read(relative, errors);
        for marker in [
            "codex plugin marketplace --help",
            "does not",
            "availability",
        ] {
            require(&text, marker, relative, errors);
        }
    }

    let checklist_relative = "docs/release-checklist.md";
    let checklist = read(checklist_relative, errors);
    for marker in [
        "automation/git-slop-v<version>",
        "Automation branch cleaned: true",
        "history/crates-io-trusted-publishing-v0.9.4.md",
    ] {
        require(&checklist, marker, checklist_relative, errors);
    }
    if checklist.contains("reserve v0.9.4") {
        errors.push(format!(
            "{checklist_relative} must keep the completed v0.9.4 migration out of the active release procedure."
        ));
    }
    let history_relative = "docs/history/crates-io-trusted-publishing-v0.9.4.md";
    let history = read(history_relative, errors);
    require(
        &history,
        "historical evidence, not an active release checklist",
        history_relative,
        errors,
    );
}
