use std::fs;
use std::path::Path;

use serde_json::Value as JsonValue;
use serde_yaml::Value as YamlValue;

pub fn validate(repo_root: &Path) -> Vec<String> {
    let mut errors = Vec::new();

    let project = load_json(
        &repo_root.join("config/github/project_config.json"),
        "config/github/project_config.json",
        &mut errors,
    );
    if let Some(project) = project {
        if project
            .pointer("/backlog_project/title")
            .and_then(JsonValue::as_str)
            != Some("git-slop")
        {
            errors.push("Project config backlog title must be git-slop.".into());
        }
        if let Some(views) = required_strings_at(
            &project,
            "/views",
            "name",
            "Project config views",
            &mut errors,
        ) {
            if views != ["Backlog", "Epics"] {
                errors.push("Project config views must be exactly Backlog and Epics.".into());
            }
        }
        if let Some(fields) = required_strings_at(
            &project,
            "/fields",
            "name",
            "Project config fields",
            &mut errors,
        ) {
            if fields != ["Status", "Priority", "Queue Order"] {
                errors.push(
                    "Project config fields must be exactly Status, Priority, and Queue Order."
                        .into(),
                );
            }
        }
    }

    let palette = load_json(
        &repo_root.join("config/labels/label_palette.json"),
        "config/labels/label_palette.json",
        &mut errors,
    );
    if let Some(palette) = palette {
        let mut managed = Vec::new();
        match palette.pointer("/labels").and_then(JsonValue::as_array) {
            Some(labels) => {
                for (index, label) in labels.iter().enumerate() {
                    let Some(label) = label.as_object() else {
                        errors.push(format!(
                            "Label palette entry {index} must be a JSON object."
                        ));
                        continue;
                    };
                    if label.get("owner").and_then(JsonValue::as_str) == Some("repo-managed") {
                        match label.get("name").and_then(JsonValue::as_str) {
                            Some(name) => managed.push(name),
                            None => errors.push(format!(
                                "Repo-managed label palette entry {index} must define a string name."
                            )),
                        }
                    }
                }
            }
            None => errors.push("Label palette must define a labels array.".into()),
        }
        managed.sort_unstable();
        if managed != ["epic", "maintenance"] {
            errors.push("Repo-managed labels must be exactly epic and maintenance.".into());
        }
    }

    validate_dependabot(repo_root, &mut errors);
    errors
}

fn validate_dependabot(repo_root: &Path, errors: &mut Vec<String>) {
    let path = repo_root.join(".github/dependabot.yml");
    let text = match fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) => {
            errors.push(format!("Unable to read .github/dependabot.yml: {error}"));
            return;
        }
    };
    let payload: YamlValue = match serde_yaml::from_str(&text) {
        Ok(payload) => payload,
        Err(error) => {
            errors.push(format!("Unable to parse .github/dependabot.yml: {error}"));
            return;
        }
    };
    let Some(updates) = payload.get("updates").and_then(YamlValue::as_sequence) else {
        errors.push("Dependabot config must define an updates sequence.".into());
        return;
    };
    let update = |ecosystem: &str| {
        updates.iter().find(|entry| {
            entry.get("package-ecosystem").and_then(YamlValue::as_str) == Some(ecosystem)
                && entry.get("directory").and_then(YamlValue::as_str) == Some("/")
        })
    };

    match update("cargo") {
        Some(cargo) => {
            let group = cargo
                .get("groups")
                .and_then(|groups| groups.get("rust-dependencies"));
            let exclusions = group
                .and_then(|group| yaml_strings(group.get("exclude-patterns")))
                .unwrap_or_default();
            if !exclusions
                .iter()
                .any(|dependency| dependency == "tiktoken-rs")
            {
                errors.push(
                    "Dependabot rust-dependencies must exclude tiktoken-rs for independent MSRV and tokenizer review."
                        .into(),
                );
            }
            let mut update_types = group
                .and_then(|group| yaml_strings(group.get("update-types")))
                .unwrap_or_default();
            update_types.sort();
            if update_types != ["minor", "patch"] {
                errors.push(
                    "Dependabot rust-dependencies must group only minor and patch updates.".into(),
                );
            }
        }
        None => errors.push("Dependabot config must define the root Cargo ecosystem.".into()),
    }

    match update("github-actions") {
        Some(actions) => {
            let setup_homebrew_ignored = actions
                .get("ignore")
                .and_then(YamlValue::as_sequence)
                .is_some_and(|entries| {
                    entries.iter().any(|entry| {
                        entry.get("dependency-name").and_then(YamlValue::as_str)
                            == Some("Homebrew/actions/setup-homebrew")
                            && entry.get("update-types").is_none()
                            && entry.get("versions").is_none()
                    })
                });
            if !setup_homebrew_ignored {
                errors.push(
                    "Dependabot must ignore setup-homebrew because release-publish.yml is generated from a source fragment."
                        .into(),
                );
            }
        }
        None => {
            errors.push("Dependabot config must define the root GitHub Actions ecosystem.".into())
        }
    }
}

fn yaml_strings(value: Option<&YamlValue>) -> Option<Vec<String>> {
    value?
        .as_sequence()?
        .iter()
        .map(|entry| entry.as_str().map(str::to_string))
        .collect()
}

fn load_json(path: &Path, label: &str, errors: &mut Vec<String>) -> Option<JsonValue> {
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) => {
            errors.push(format!("Unable to read {label}: {error}"));
            return None;
        }
    };
    match serde_json::from_str(&text) {
        Ok(value) => Some(value),
        Err(error) => {
            errors.push(format!("Unable to parse {label}: {error}"));
            None
        }
    }
}

fn required_strings_at<'a>(
    value: &'a JsonValue,
    pointer: &str,
    field: &str,
    label: &str,
    errors: &mut Vec<String>,
) -> Option<Vec<&'a str>> {
    let Some(entries) = value.pointer(pointer).and_then(JsonValue::as_array) else {
        errors.push(format!("{label} must be a JSON array."));
        return None;
    };
    let mut strings = Vec::with_capacity(entries.len());
    for (index, entry) in entries.iter().enumerate() {
        match entry.get(field).and_then(JsonValue::as_str) {
            Some(value) => strings.push(value),
            None => errors.push(format!(
                "{label} entry {index} must define a string {field}."
            )),
        }
    }
    if strings.len() == entries.len() {
        Some(strings)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn repository_overlays_pass() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
        assert_eq!(validate(root), Vec::<String>::new());
    }

    #[test]
    fn malformed_required_entries_fail_closed() {
        let temp = TempDir::new().unwrap();
        let root = temp.path();
        fs::create_dir_all(root.join("config/github")).unwrap();
        fs::create_dir_all(root.join("config/labels")).unwrap();
        fs::write(
            root.join("config/github/project_config.json"),
            r#"{
                "backlog_project": {"title": "git-slop"},
                "views": [{"name": "Backlog"}, {"name": "Epics"}, {}],
                "fields": [
                    {"name": "Status"},
                    {"name": "Priority"},
                    {"name": "Queue Order"}
                ]
            }"#,
        )
        .unwrap();
        fs::write(
            root.join("config/labels/label_palette.json"),
            r#"{
                "labels": [
                    {"owner": "repo-managed", "name": "epic"},
                    {"owner": "repo-managed"},
                    {"owner": "repo-managed", "name": "maintenance"}
                ]
            }"#,
        )
        .unwrap();

        let errors = validate(root);
        assert!(errors.iter().any(|error| error.contains("views entry 2")));
        assert!(
            errors
                .iter()
                .any(|error| error.contains("label palette entry 1"))
        );
    }

    #[test]
    fn unsafe_dependabot_grouping_fails_closed() {
        let temp = TempDir::new().unwrap();
        fs::create_dir_all(temp.path().join(".github")).unwrap();
        fs::write(
            temp.path().join(".github/dependabot.yml"),
            r#"version: 2
updates:
  - package-ecosystem: cargo
    directory: /
    groups:
      rust-dependencies:
        patterns: ["*"]
  - package-ecosystem: github-actions
    directory: /
    groups:
      github-actions:
        patterns: ["*"]
"#,
        )
        .unwrap();
        let mut errors = Vec::new();
        validate_dependabot(temp.path(), &mut errors);
        assert!(
            errors
                .iter()
                .any(|error| error.contains("exclude tiktoken-rs"))
        );
        assert!(errors.iter().any(|error| error.contains("minor and patch")));
        assert!(
            errors
                .iter()
                .any(|error| error.contains("ignore setup-homebrew"))
        );
    }
}
