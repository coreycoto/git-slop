use anyhow::{Context, Result, bail};
use globset::Glob;
use serde_json::{Map, Value, json};

use crate::model::Classification;

pub(super) fn validate_path_command(
    mapping: &Map<String, Value>,
    path: &str,
    index: usize,
) -> Result<()> {
    if mapping
        .get("command")
        .and_then(Value::as_str)
        .is_none_or(|command| command.trim().is_empty())
    {
        bail!("{path}[{index}].command must be a non-empty string");
    }
    Ok(())
}

pub(super) fn validate_generated_override(
    mapping: &Map<String, Value>,
    path: &str,
    index: usize,
) -> Result<()> {
    if let Some(globs) = mapping.get("generated_source_globs") {
        let Some(globs) = globs.as_array() else {
            bail!("{path}[{index}].generated_source_globs must be an array");
        };
        for (glob_index, glob) in globs.iter().enumerate() {
            let Some(glob) = glob.as_str() else {
                bail!("{path}[{index}].generated_source_globs[{glob_index}] must be a string");
            };
            Glob::new(glob).with_context(|| {
                format!("{path}[{index}].generated_source_globs[{glob_index}] is not a valid glob")
            })?;
        }
    }
    for key in ["generator_command", "verification_command"] {
        if mapping
            .get(key)
            .is_some_and(|value| value.as_str().is_none_or(|value| value.trim().is_empty()))
        {
            bail!("{path}[{index}].{key} must be a non-empty string");
        }
    }
    Ok(())
}

pub(super) fn path_command_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["path_glob", "command"],
        "properties": {
            "path_glob": {"type": "string", "minLength": 1, "description": "Repository path glob."},
            "command": {"type": "string", "minLength": 1, "description": "Focused verification command."}
        }
    })
}

pub(super) fn path_override_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["glob"],
        "properties": {
            "glob": {"type": "string", "minLength": 1},
            "classification": {"type": "string", "enum": Classification::values()},
            "profile": {"type": "string", "enum": ["agent_context", "data_context"]},
            "language": {"type": "string", "minLength": 1},
            "verification_applicability": {"type": "string", "enum": ["auto", "applicable", "not_applicable"]},
            "generated_source_globs": {"type": "array", "items": {"type": "string", "minLength": 1}},
            "generator_command": {"type": "string", "minLength": 1},
            "verification_command": {"type": "string", "minLength": 1}
        },
        "anyOf": [
            {"required": ["classification"]},
            {"required": ["profile"]},
            {"required": ["language"]},
            {"required": ["verification_applicability"]},
            {"required": ["generated_source_globs"]},
            {"required": ["generator_command"]},
            {"required": ["verification_command"]}
        ]
    })
}

#[cfg(test)]
mod tests {
    use std::fs;

    use serde_json::json;
    use tempfile::tempdir;

    use super::super::{config_path, load};

    #[test]
    fn verification_commands_accept_non_empty_entries_and_reject_blank_entries() {
        let repository = tempdir().expect("temporary repository");
        let path = config_path(repository.path());
        fs::create_dir_all(path.parent().expect("config parent")).expect("config directory");
        fs::write(
            &path,
            serde_yaml::to_string(&json!({
                "schema_version": 2,
                "verification": {"commands": ["cargo test --locked"]}
            }))
            .expect("serialize config"),
        )
        .expect("config");
        let normalized = load(repository.path()).expect("valid command");
        assert_eq!(
            normalized["verification"]["commands"],
            json!(["cargo test --locked"])
        );

        fs::write(
            &path,
            "schema_version: 2\nverification:\n  commands:\n    - '   '\n",
        )
        .expect("config");
        let error = load(repository.path()).expect_err("blank command must fail closed");
        assert!(
            error.to_string().contains("must be a non-empty string"),
            "{error:#}"
        );
    }
}
