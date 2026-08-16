pub fn validate(repo_root: &Path, require_codex_cli: bool) -> Vec<String> {
    let mut errors = Vec::new();

    validate_codex_config(repo_root, &mut errors);
    validate_execpolicy(repo_root, require_codex_cli, &mut errors);
    validate_marketplaces(repo_root, &mut errors);
    validate_product_plugin(repo_root, &mut errors);
    validate_removed_surfaces(repo_root, &mut errors);
    validate_agents(repo_root, &mut errors);
    validate_workflow_assets(repo_root, &mut errors);
    validate_guidance(repo_root, &mut errors);
    runtime_manifest::validate_agent_plugin_wrapper(repo_root, &mut errors);
    runtime_workflows::validate_agent_plugin_workflows(repo_root, &mut errors);
    validate_release_workflow(repo_root, &mut errors);
    validate_product_documentation(repo_root, &mut errors);

    errors
}

fn validate_codex_config(repo_root: &Path, errors: &mut Vec<String>) {
    let config_path = ".codex/config.toml";
    if let Some(payload) = load_toml(repo_root, config_path, errors) {
        if toml_string(&payload, "approval_policy") != Some("on-request") {
            errors.push(".codex/config.toml must default approval_policy to on-request.".into());
        }
        if toml_string(&payload, "sandbox_mode") != Some("workspace-write") {
            errors.push(".codex/config.toml must default sandbox_mode to workspace-write.".into());
        }
        if payload.get("profile").is_some() {
            errors.push(
                ".codex/config.toml must not define the legacy profile selector; select a \
                 standalone profile with --profile."
                    .into(),
            );
        }
        if payload.get("profiles").is_some() {
            errors.push(
                ".codex/config.toml must not define legacy [profiles.*] tables; use standalone \
                 .codex/<profile>.config.toml files."
                    .into(),
            );
        }
    }

    for (profile_name, expected_sandbox_mode) in CI_PROFILES {
        let relative = format!(".codex/{profile_name}.config.toml");
        let Some(payload) = load_toml(repo_root, &relative, errors) else {
            continue;
        };
        if toml_string(&payload, "approval_policy") != Some("never") {
            errors.push(format!(
                "{relative} must set top-level approval_policy to never."
            ));
        }
        if toml_string(&payload, "sandbox_mode") != Some(expected_sandbox_mode) {
            errors.push(format!(
                "{relative} must set top-level sandbox_mode to {expected_sandbox_mode}."
            ));
        }
    }
}

fn validate_execpolicy(repo_root: &Path, require_codex_cli: bool, errors: &mut Vec<String>) {
    let relative_rule_path = ".codex/rules/git.rules";
    let rule_path = repo_root.join(relative_rule_path);
    if !rule_path.is_file() {
        errors.push(".codex/rules/git.rules is missing.".into());
        return;
    }

    if !command_on_path("codex") {
        if require_codex_cli {
            errors.push("codex CLI is required but not installed.".into());
        }
        return;
    }

    const COMMANDS: [&[&str]; 3] = [
        &["git", "push", "origin", "main"],
        &["gh", "release", "create", "v1.2.3"],
        &["gh", "pr", "merge", "123", "--squash"],
    ];

    for checked_command in COMMANDS {
        let output = Command::new("codex")
            .args(["execpolicy", "check", "--rules"])
            .arg(&rule_path)
            .arg("--")
            .args(checked_command)
            .current_dir(repo_root)
            .output();

        let rendered_command = checked_command.join(" ");
        let output = match output {
            Ok(output) => output,
            Err(error) => {
                errors.push(format!(
                    "execpolicy check failed to start for {rendered_command}: {error}"
                ));
                continue;
            }
        };

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
            let stdout = String::from_utf8_lossy(&output.stdout).trim().to_owned();
            let detail = if stderr.is_empty() { stdout } else { stderr };
            errors.push(format!(
                "execpolicy check failed for {rendered_command}: {detail}"
            ));
            continue;
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        match require_prompt_decision(&stdout) {
            Ok(()) => {}
            Err(error) => errors.push(format!(
                "execpolicy check returned an unsafe result for {rendered_command}: {error}"
            )),
        }
    }
}

fn parse_execpolicy_decision(stdout: &str) -> Result<String, String> {
    let trimmed = stdout.trim();
    if trimmed.is_empty() {
        return Err("stdout was empty".into());
    }
    let payload: JsonValue = serde_json::from_str(trimmed)
        .map_err(|error| format!("stdout was not valid JSON: {error}"))?;
    payload
        .get("decision")
        .and_then(JsonValue::as_str)
        .map(str::to_owned)
        .ok_or_else(|| "JSON output did not contain a string decision".into())
}

fn require_prompt_decision(stdout: &str) -> Result<(), String> {
    let decision = parse_execpolicy_decision(stdout)?;
    if decision == EXPECTED_EXEC_POLICY_DECISION {
        Ok(())
    } else {
        Err(format!(
            "decision was {decision:?}; expected {EXPECTED_EXEC_POLICY_DECISION:?}"
        ))
    }
}
