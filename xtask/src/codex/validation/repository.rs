fn validate_removed_surfaces(repo_root: &Path, errors: &mut Vec<String>) {
    for relative in REMOVED_CONSUMER_PATHS {
        if repo_root.join(relative).exists() {
            errors.push(format!(
                "{relative} should have been removed from the Rust-only maintainer surface."
            ));
        }
    }

    match crate::distribution::repository_owned_py_files(repo_root) {
        Ok(paths) => errors.extend(
            paths
                .into_iter()
                .map(|path| format!("Repository-owned .py file must be removed: {path}.")),
        ),
        Err(error) => errors.push(error),
    }
}

fn validate_agents(repo_root: &Path, errors: &mut Vec<String>) {
    for agent in AGENTS {
        let Some(payload) = read_text(repo_root, agent.path, errors) else {
            errors.push(format!(
                "Missing custom agent file for {}: {}.",
                agent.name, agent.path
            ));
            continue;
        };
        if payload.contains("[[skills.config]]") {
            errors.push(format!("{} must not bind local plugin paths.", agent.path));
        }
        for forbidden in REMOVED_LOCAL_PLUGIN_REFERENCES {
            if payload.contains(forbidden) {
                errors.push(format!("{} must not reference {forbidden}.", agent.path));
            }
        }
        for skill in agent.skills {
            if !payload.contains(skill) {
                errors.push(format!("{} must mention {skill}.", agent.path));
            }
        }
    }
}

fn validate_workflow_assets(repo_root: &Path, errors: &mut Vec<String>) {
    for workflow in WORKFLOWS {
        let workflow_path = format!(".github/workflows/{}", workflow.name);
        let workflow_text = read_text(repo_root, &workflow_path, errors);
        let prompt_text = read_text(repo_root, workflow.prompt, errors);
        let schema_text = read_text(repo_root, workflow.schema, errors);

        if let Some(prompt_text) = prompt_text {
            if !prompt_text.contains(workflow.skill) {
                errors.push(format!(
                    "{} must mention {}.",
                    workflow.prompt, workflow.skill
                ));
            }
            if !prompt_text.contains(workflow.agent_file) {
                errors.push(format!(
                    "{} must mention {}.",
                    workflow.prompt, workflow.agent_file
                ));
            }
            for forbidden in REMOVED_LOCAL_PLUGIN_REFERENCES {
                if prompt_text.contains(forbidden) {
                    errors.push(format!(
                        "{} must not reference {forbidden}.",
                        workflow.prompt
                    ));
                }
            }
        }

        if let Some(schema_text) = schema_text {
            if let Err(error) = serde_json::from_str::<JsonValue>(&schema_text) {
                errors.push(format!("Unable to parse {}: {error}", workflow.schema));
            }
        }

        if workflow.uses_agent_plugins {
            if let Some(workflow_text) = workflow_text {
                if !workflow_text.contains(workflow.prompt) {
                    errors.push(format!(
                        "{} must invoke prompt file {}.",
                        workflow.name, workflow.prompt
                    ));
                }
                if !workflow_text.contains(workflow.schema) {
                    errors.push(format!(
                        "{} must invoke output schema {}.",
                        workflow.name, workflow.schema
                    ));
                }
            }
        }
    }
}
