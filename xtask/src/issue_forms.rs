use std::fs;
use std::path::Path;

use serde_yaml::{Mapping, Value};

const FORMS: [(&str, &str, &str); 5] = [
    ("epic.yml", "Epic: ", "epic"),
    ("maintenance.yml", "Maintenance: ", "maintenance"),
    ("enhancement.yml", "Enhancement: ", "enhancement"),
    ("research.yml", "Research: ", "question"),
    ("bug.yml", "Bug: ", "bug"),
];

pub fn validate(repo_root: &Path) -> Vec<String> {
    let mut errors = Vec::new();
    let template_root = repo_root.join(".github/ISSUE_TEMPLATE");

    for (filename, expected_title, expected_label) in FORMS {
        let path = template_root.join(filename);
        let label = format!(".github/ISSUE_TEMPLATE/{filename}");
        if let Ok(text) = fs::read_to_string(&path) {
            if let Some(literal) = shipped_version_literal(&text) {
                errors.push(format!(
                    "{label} must use version-neutral examples such as 0.x.y, not {literal:?}."
                ));
            }
            if let Some(literal) = concrete_quarter_literal(&text) {
                errors.push(format!(
                    "{label} must use the quarter placeholder YYYY QN, not {literal:?}."
                ));
            }
        }
        let Some(payload) = load_mapping(&path, &label, &mut errors) else {
            continue;
        };

        if string_field(&payload, "title") != Some(expected_title) {
            errors.push(format!(
                ".github/ISSUE_TEMPLATE/{filename} must use title prefix {expected_title:?}."
            ));
        }
        let has_label = payload
            .get(Value::String("labels".into()))
            .and_then(Value::as_sequence)
            .is_some_and(|labels| {
                labels
                    .iter()
                    .any(|label| label.as_str() == Some(expected_label))
            });
        if !has_label {
            errors.push(format!(
                ".github/ISSUE_TEMPLATE/{filename} must include label {expected_label:?}."
            ));
        }
        let has_body = payload
            .get(Value::String("body".into()))
            .and_then(Value::as_sequence)
            .is_some_and(|body| !body.is_empty());
        if !has_body {
            errors.push(format!(
                ".github/ISSUE_TEMPLATE/{filename} must define a non-empty body."
            ));
        }
        if filename == "bug.yml" {
            let entries = payload
                .get(Value::String("body".into()))
                .and_then(Value::as_sequence)
                .into_iter()
                .flatten()
                .filter_map(Value::as_mapping)
                .collect::<Vec<_>>();
            let ids = entries
                .iter()
                .filter_map(|entry| string_field(entry, "id"))
                .collect::<std::collections::BTreeSet<_>>();
            for required in [
                "version",
                "build-info",
                "platform",
                "installation",
                "command-error",
                "doctor-bundle",
            ] {
                if !ids.contains(required) {
                    errors.push(format!(
                        ".github/ISSUE_TEMPLATE/bug.yml must collect {required:?}."
                    ));
                }
            }
            let doctor_bundle_is_optional = entries.iter().any(|entry| {
                string_field(entry, "id") == Some("doctor-bundle")
                    && entry
                        .get(Value::String("validations".into()))
                        .and_then(Value::as_mapping)
                        .and_then(|validations| validations.get(Value::String("required".into())))
                        .and_then(Value::as_bool)
                        == Some(false)
            });
            if !doctor_bundle_is_optional {
                errors.push(
                    ".github/ISSUE_TEMPLATE/bug.yml must keep doctor-bundle optional.".into(),
                );
            }
        }
    }

    let config_path = template_root.join("config.yml");
    if let Some(payload) = load_mapping(
        &config_path,
        ".github/ISSUE_TEMPLATE/config.yml",
        &mut errors,
    ) {
        let first_link = payload
            .get(Value::String("contact_links".into()))
            .and_then(Value::as_sequence)
            .and_then(|links| links.first())
            .and_then(Value::as_mapping);
        match first_link {
            Some(link) => {
                if string_field(link, "name") != Some("Contributing Guide") {
                    errors.push(
                        ".github/ISSUE_TEMPLATE/config.yml first contact link must be named \
                         Contributing Guide."
                            .into(),
                    );
                }
                if !string_field(link, "url").is_some_and(|url| url.contains("CONTRIBUTING.md")) {
                    errors.push(
                        ".github/ISSUE_TEMPLATE/config.yml first contact link must point to \
                         CONTRIBUTING.md."
                            .into(),
                    );
                }
            }
            None => errors.push(
                ".github/ISSUE_TEMPLATE/config.yml must define at least one contact link.".into(),
            ),
        }
        let has_troubleshooting = payload
            .get(Value::String("contact_links".into()))
            .and_then(Value::as_sequence)
            .into_iter()
            .flatten()
            .filter_map(Value::as_mapping)
            .any(|link| {
                string_field(link, "url").is_some_and(|url| url.contains("docs/troubleshooting.md"))
            });
        if !has_troubleshooting {
            errors.push(
                ".github/ISSUE_TEMPLATE/config.yml must link to docs/troubleshooting.md.".into(),
            );
        }
    }

    errors
}

fn shipped_version_literal(text: &str) -> Option<&str> {
    text.split(|character: char| !(character.is_ascii_alphanumeric() || character == '.'))
        .find(|token| {
            let parts = token.split('.').collect::<Vec<_>>();
            parts.len() == 3
                && parts.iter().all(|part| {
                    !part.is_empty() && part.chars().all(|value| value.is_ascii_digit())
                })
        })
}

fn concrete_quarter_literal(text: &str) -> Option<String> {
    let words = text
        .split(|character: char| !character.is_ascii_alphanumeric())
        .filter(|word| !word.is_empty())
        .collect::<Vec<_>>();
    words.windows(2).find_map(|pair| {
        let year = pair[0];
        let quarter = pair[1];
        (year.len() == 4
            && year.starts_with("20")
            && year.chars().all(|value| value.is_ascii_digit())
            && matches!(quarter, "Q1" | "Q2" | "Q3" | "Q4"))
        .then(|| format!("{year} {quarter}"))
    })
}

fn load_mapping(path: &Path, label: &str, errors: &mut Vec<String>) -> Option<Mapping> {
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) => {
            errors.push(format!("Unable to read {label}: {error}"));
            return None;
        }
    };
    match serde_yaml::from_str::<Value>(&text) {
        Ok(Value::Mapping(mapping)) => Some(mapping),
        Ok(_) => {
            errors.push(format!("{label} must contain a YAML mapping."));
            None
        }
        Err(error) => {
            errors.push(format!("Unable to parse {label}: {error}"));
            None
        }
    }
}

fn string_field<'a>(mapping: &'a Mapping, key: &str) -> Option<&'a str> {
    mapping
        .get(Value::String(key.into()))
        .and_then(Value::as_str)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn repository_issue_forms_pass() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
        assert_eq!(validate(root), Vec::<String>::new());
    }

    #[test]
    fn rejects_wrong_prefix_and_missing_contact_link() {
        let temp = TempDir::new().unwrap();
        let root = temp.path();
        let templates = root.join(".github/ISSUE_TEMPLATE");
        fs::create_dir_all(&templates).unwrap();
        for (filename, title, label) in FORMS {
            let title = if filename == "bug.yml" {
                "Wrong: "
            } else {
                title
            };
            fs::write(
                templates.join(filename),
                format!("title: '{title}'\nlabels: ['{label}']\nbody:\n  - type: markdown\n"),
            )
            .unwrap();
        }
        fs::write(templates.join("config.yml"), "contact_links: []\n").unwrap();

        let errors = validate(root);
        assert!(
            errors
                .iter()
                .any(|error| error.contains("bug.yml must use title prefix"))
        );
        assert!(
            errors
                .iter()
                .any(|error| error.contains("must define at least one contact link"))
        );
    }

    #[test]
    fn rejects_shipped_versions_and_concrete_quarters() {
        assert_eq!(
            shipped_version_literal("placeholder: 0.15.0"),
            Some("0.15.0")
        );
        assert_eq!(
            concrete_quarter_literal("placeholder: 2026 Q2").as_deref(),
            Some("2026 Q2")
        );
        assert_eq!(shipped_version_literal("placeholder: 0.x.y"), None);
        assert_eq!(concrete_quarter_literal("placeholder: YYYY QN"), None);
    }
}
