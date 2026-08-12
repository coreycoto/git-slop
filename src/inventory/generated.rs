use globset::Glob;
use serde_json::{Value, json};

use super::CompiledPathOverride;

fn marker(text: &str) -> Option<(String, Option<String>)> {
    text.lines().take(3).find_map(|line| {
        let normalized = line.trim().trim_start_matches(['#', '/', '*', ' ']).trim();
        let lower = normalized.to_ascii_lowercase();
        let marker = lower.find("@generated from ")?;
        let descriptor = normalized[marker + "@generated from ".len()..].trim();
        let (source, command) = descriptor
            .split_once(" by ")
            .map_or((descriptor, None), |(source, command)| {
                (source, Some(command))
            });
        Some((
            source.trim().to_owned(),
            command.map(|value| value.trim().to_owned()),
        ))
    })
}

pub(super) fn generated_provenance(text: &str, tracked_paths: &[String]) -> (Vec<String>, Value) {
    let Some((source, generator_command)) = marker(text) else {
        return (
            Vec::new(),
            json!({
                "source_paths": [],
                "source_globs": [],
                "generator_command": null,
                "verification_command": null
            }),
        );
    };
    let generated_from = source.clone();
    let is_glob = source.contains(['*', '?', '[']);
    let source_globs = if is_glob {
        vec![source.clone()]
    } else {
        Vec::new()
    };
    let mut source_paths = if is_glob {
        Glob::new(&source)
            .ok()
            .map(|glob| {
                let matcher = glob.compile_matcher();
                tracked_paths
                    .iter()
                    .filter(|path| matcher.is_match(path))
                    .cloned()
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default()
    } else if tracked_paths.iter().any(|path| path == &source) {
        vec![source]
    } else {
        Vec::new()
    };
    source_paths.sort();
    source_paths.dedup();
    let verification_command = generator_command.as_deref().and_then(|command| {
        command
            .contains("generate-release-workflow")
            .then(|| format!("{command} --check"))
    });
    let provenance = json!({
        "source_paths": source_paths,
        "source_globs": source_globs,
        "generator_command": generator_command,
        "verification_command": verification_command
    });
    let generated_from = if source_paths.is_empty() {
        vec![generated_from]
    } else {
        source_paths
    };
    (generated_from, provenance)
}

pub(super) fn configured_generated_provenance(
    path: &str,
    overrides: &[CompiledPathOverride],
    tracked_paths: &[String],
) -> Option<(Vec<String>, Value)> {
    let mapping = overrides.iter().rev().find(|mapping| {
        mapping.matcher.is_match(path)
            && (!mapping.generated_source_globs.is_empty()
                || mapping.generator_command.is_some()
                || mapping.verification_command.is_some())
    })?;
    let mut source_paths = mapping
        .generated_source_globs
        .iter()
        .filter_map(|pattern| Glob::new(pattern).ok())
        .map(|pattern| pattern.compile_matcher())
        .flat_map(|matcher| {
            tracked_paths
                .iter()
                .filter(move |candidate| matcher.is_match(candidate))
                .cloned()
        })
        .collect::<Vec<_>>();
    source_paths.sort();
    source_paths.dedup();
    let generated_from = if source_paths.is_empty() {
        mapping.generated_source_globs.clone()
    } else {
        source_paths.clone()
    };
    Some((
        generated_from,
        json!({
            "source_paths": source_paths,
            "source_globs": mapping.generated_source_globs,
            "generator_command": mapping.generator_command,
            "verification_command": mapping.verification_command
        }),
    ))
}
