fn markdown_table_names(document: &str, heading: &str) -> std::collections::BTreeSet<String> {
    let Some(section) = document.split(heading).nth(1) else {
        return std::collections::BTreeSet::new();
    };
    section
        .split("\n## ")
        .next()
        .unwrap_or(section)
        .lines()
        .filter_map(|line| line.strip_prefix("| `"))
        .filter_map(|line| line.split('`').next())
        .map(str::to_string)
        .collect()
}

fn validate_action_documentation(repo_root: &Path, errors: &mut Vec<String>) {
    let action = match fs::read_to_string(repo_root.join("action.yml"))
        .ok()
        .and_then(|text| serde_yaml::from_str::<YamlValue>(&text).ok())
    {
        Some(action) => action,
        None => {
            errors.push("action.yml must be readable YAML for documentation validation.".into());
            return;
        }
    };
    let docs = match fs::read_to_string(repo_root.join("docs/github-action.md")) {
        Ok(docs) => docs,
        Err(error) => {
            errors.push(format!("Unable to read docs/github-action.md: {error}"));
            return;
        }
    };
    for (field, heading) in [("inputs", "## Inputs"), ("outputs", "## Outputs")] {
        let expected = action[field]
            .as_mapping()
            .into_iter()
            .flat_map(|mapping| mapping.keys())
            .filter_map(YamlValue::as_str)
            .map(str::to_string)
            .collect::<std::collections::BTreeSet<_>>();
        let documented = markdown_table_names(&docs, heading);
        if expected != documented {
            errors.push(format!(
                "docs/github-action.md {field} table must exactly match action.yml: expected={expected:?}, documented={documented:?}"
            ));
        }
    }
}
