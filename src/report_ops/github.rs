use super::*;
use crate::text::{github_message_escape, github_property_escape};
use std::io::Read;

fn collect_prompt_paths(value: &Value, paths: &mut Vec<String>) {
    match value {
        Value::Object(object) => {
            for (key, value) in object {
                if matches!(key.as_str(), "path" | "source_path" | "target_path") {
                    if let Some(path) = value.as_str() {
                        if !paths.iter().any(|existing| existing == path) {
                            paths.push(path.to_string());
                        }
                    }
                } else if matches!(key.as_str(), "scope_paths" | "in_scope") {
                    if let Some(values) = value.as_array() {
                        for path in values.iter().filter_map(Value::as_str) {
                            if !paths.iter().any(|existing| existing == path) {
                                paths.push(path.to_string());
                            }
                        }
                    }
                }
                collect_prompt_paths(value, paths);
            }
        }
        Value::Array(values) => {
            for value in values {
                collect_prompt_paths(value, paths);
            }
        }
        _ => {}
    }
}

fn bounded_repository_text(root: &Path, relative: &str, byte_limit: usize) -> Option<Value> {
    let relative_path = Path::new(relative);
    if relative_path.as_os_str().is_empty()
        || relative_path.is_absolute()
        || relative_path.components().any(|component| {
            matches!(
                component,
                std::path::Component::ParentDir | std::path::Component::RootDir
            )
        })
    {
        return None;
    }
    let absolute = root.join(relative_path);
    let metadata = fs::symlink_metadata(&absolute).ok()?;
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return None;
    }
    let mut bytes = Vec::with_capacity(byte_limit.saturating_add(1));
    std::io::Read::read_to_end(
        &mut fs::File::open(absolute)
            .ok()?
            .take(u64::try_from(byte_limit.saturating_add(1)).ok()?),
        &mut bytes,
    )
    .ok()?;
    let truncated = bytes.len() > byte_limit;
    bytes.truncate(byte_limit);
    Some(json!({
        "path": relative,
        "excerpt": String::from_utf8_lossy(&bytes),
        "bytes_returned": bytes.len(),
        "truncated": truncated
    }))
}

fn applicable_guidance_paths(candidate_paths: &[String]) -> Vec<String> {
    let mut paths = vec![
        "AGENTS.md".to_string(),
        "CONTRIBUTING.md".to_string(),
        "README.md".to_string(),
    ];
    for candidate in candidate_paths {
        let mut parent = Path::new(candidate).parent();
        while let Some(directory) = parent {
            if directory.as_os_str().is_empty() {
                break;
            }
            let path = directory
                .join("AGENTS.md")
                .to_string_lossy()
                .replace('\\', "/");
            if !paths.contains(&path) {
                paths.push(path);
            }
            parent = directory.parent();
        }
    }
    paths
}

fn complete_repository_text(root: &Path, relative: &str, remaining: usize) -> Option<Value> {
    let absolute = root.join(relative);
    let metadata = fs::symlink_metadata(&absolute).ok()?;
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return None;
    }
    let bytes = fs::read(absolute).ok()?;
    if bytes.len() > remaining {
        return Some(json!({
            "path": relative,
            "included": false,
            "bytes": bytes.len(),
            "reason": "guidance_budget_exceeded"
        }));
    }
    Some(json!({
        "path": relative,
        "included": true,
        "bytes": bytes.len(),
        "text": String::from_utf8_lossy(&bytes)
    }))
}

fn repository_context(payload: &Value, report: &Value, root: &Path, excerpt_bytes: usize) -> Value {
    let byte_limit = excerpt_bytes.clamp(256, 4096);
    let mut candidate_paths = Vec::new();
    collect_prompt_paths(payload, &mut candidate_paths);
    let source_excerpts = candidate_paths
        .iter()
        .filter_map(|path| bounded_repository_text(root, path, byte_limit))
        .take(10)
        .collect::<Vec<_>>();
    let guidance_budget = 65_536usize;
    let mut guidance_bytes = 0usize;
    let guidance = applicable_guidance_paths(&candidate_paths)
        .iter()
        .filter_map(|path| {
            let value = complete_repository_text(
                root,
                path,
                guidance_budget.saturating_sub(guidance_bytes),
            )?;
            if value.get("included").and_then(Value::as_bool) == Some(true) {
                guidance_bytes += value
                    .get("bytes")
                    .and_then(Value::as_u64)
                    .unwrap_or_default() as usize;
            }
            Some(value)
        })
        .collect::<Vec<_>>();
    let guidance_complete = guidance
        .iter()
        .all(|item| item.get("included").and_then(Value::as_bool) == Some(true));
    let configured_commands = string_array(report.pointer("/config/verification/commands"));
    let verification_commands =
        super::verification::from_worktree(root, &candidate_paths, &configured_commands);
    json!({
        "included": true,
        "execution_usable": guidance_complete,
        "reason": "explicit_opt_in",
        "source_excerpts": source_excerpts,
        "guidance": guidance,
        "verification_commands": verification_commands,
        "truncation": {
            "per_file_byte_limit": byte_limit,
            "source_file_limit": 10,
            "source_candidate_count": candidate_paths.len(),
            "source_returned_count": source_excerpts.len(),
            "guidance_total_byte_budget": guidance_budget,
            "guidance_bytes_returned": guidance_bytes,
            "guidance_complete": guidance_complete,
            "guidance_returned_count": guidance.len()
        }
    })
}

pub fn health_json_payload(report: &Value) -> Value {
    json!({
        "schema_version": 1,
        "command": "health",
        "report": {
            "schema_version": report.get("schema_version").cloned().unwrap_or(Value::Null),
            "analyzer": report.get("analyzer").cloned().unwrap_or(Value::Null),
            "repo": report.get("repo").cloned().unwrap_or(Value::Null),
            "scope": report.get("scope").cloned().unwrap_or(Value::Null),
        },
        "health": report.get("health").cloned().unwrap_or_else(|| json!({"findings": []})),
        "collection_metadata": report.pointer("/collection_metadata/health.findings").cloned().unwrap_or_else(|| json!({
            "total": report.pointer("/health/findings").and_then(Value::as_array).map(Vec::len).unwrap_or_default(),
            "returned": report.pointer("/health/findings").and_then(Value::as_array).map(Vec::len).unwrap_or_default(),
            "limit": null,
            "truncated": false
        }))
    })
}

fn github_annotation_level(severity: &str) -> &'static str {
    if severity.eq_ignore_ascii_case("notice") {
        "notice"
    } else if severity.eq_ignore_ascii_case("error") {
        "error"
    } else {
        "warning"
    }
}

pub fn render_github_annotations(report: &Value, max_annotations: usize) -> String {
    let lines: Vec<String> = array_at(report, &["health", "findings"])
        .iter()
        .take(max_annotations)
        .map(|finding| {
            let severity = string(finding.get("severity"));
            let command = github_annotation_level(&severity);
            let path = github_property_escape(&string(finding.get("path")));
            let title = github_property_escape(&string(finding.get("title")));
            let mut message = string(finding.get("message"));
            let next_command = string(finding.get("next_command"));
            if !next_command.is_empty() {
                if !message.is_empty() && !message.chars().last().is_some_and(char::is_whitespace) {
                    message.push(' ');
                }
                message.push_str("Next: ");
                message.push_str(&next_command);
            }
            format!(
                "::{command} file={path},title={title}::{}",
                github_message_escape(&message)
            )
        })
        .collect();
    if lines.is_empty() {
        String::new()
    } else {
        format!("{}\n", lines.join("\n"))
    }
}

pub fn write_prompt_pack(
    command: &str,
    payload: &Value,
    report: &Value,
    output_dir: &Path,
    repository_root: Option<&Path>,
    excerpt_bytes: usize,
    force: bool,
) -> Result<()> {
    if output_dir.exists() {
        if !output_dir.is_dir() {
            bail!(
                "Prompt pack path is not a directory: {}",
                output_dir.display()
            );
        }
        let empty = fs::read_dir(output_dir)?.next().is_none();
        if !empty && !force {
            bail!(
                "Prompt pack path already exists and is not empty; pass --force to replace it: {}",
                output_dir.display()
            );
        }
    }
    let parent = output_dir.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)?;
    let name = output_dir
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("prompt-pack");
    let temporary = parent.join(format!(".{name}-{}-tmp", std::process::id()));
    if temporary.exists() {
        fs::remove_dir_all(&temporary)?;
    }
    fs::create_dir(&temporary)?;
    let report_digest = serde_json::to_vec(report)
        .map(|bytes| hex::encode(Sha256::digest(bytes)))
        .unwrap_or_default();
    let mut selected_paths = Vec::new();
    collect_prompt_paths(payload, &mut selected_paths);
    let mut report_files = array_at(report, &["files"]).iter().collect::<Vec<_>>();
    report_files.sort_by_key(|file| {
        let path = file.get("path").and_then(Value::as_str).unwrap_or_default();
        selected_paths
            .iter()
            .position(|selected| selected == path)
            .unwrap_or(usize::MAX)
    });
    let evidence_excerpts = report_files
        .into_iter()
        .take(20)
        .map(|file| {
            json!({
                "path": file.get("path"),
                "slop_score": file.get("slop_score"),
                "slop_band": file.get("slop_band"),
                "context_band": file.get("context_band"),
                "reason_codes": file.get("reason_codes"),
                "content_fingerprint": file.get("content_fingerprint")
            })
        })
        .collect::<Vec<_>>();
    let repository_context = repository_root.map_or_else(
        || {
            json!({
                "included": false,
                "reason": "not_requested",
                "source_excerpts": [],
                "guidance": [],
                "verification_commands": [],
                "truncation": {"per_file_byte_limit": 0, "file_limit": 0}
            })
        },
        |root| repository_context(payload, report, root, excerpt_bytes),
    );
    let context = json!({
        "prompt_pack_version": 1,
        "command": command,
        "payload": payload,
        "provenance": {
            "analyzer_version": report.pointer("/analyzer/version").cloned().unwrap_or(Value::Null),
            "analysis_contract_version": report.pointer("/analyzer/analysis_contract_version").cloned().unwrap_or(Value::Null),
            "analysis_config_digest": report.pointer("/analyzer/analysis_config_digest").cloned().unwrap_or(Value::Null),
            "evidence_config_digest": report.pointer("/analyzer/evidence_config_digest").cloned().unwrap_or(Value::Null),
            "policy_config_digest": report.pointer("/analyzer/policy_config_digest").cloned().unwrap_or(Value::Null),
            "presentation_config_digest": report.pointer("/analyzer/presentation_config_digest").cloned().unwrap_or(Value::Null),
            "head_sha": report.pointer("/repo/head_sha").cloned().unwrap_or(Value::Null),
            "generated_at": report.get("generated_at").cloned().unwrap_or(Value::Null),
            "analyzed_revision_at": report.get("analyzed_revision_at").cloned().unwrap_or(Value::Null),
            "worktree_state_digest": report.pointer("/repo/worktree_state_digest").cloned().unwrap_or(Value::Null),
            "analyzed_content_digest": report.pointer("/repo/analyzed_content_digest").cloned().unwrap_or(Value::Null),
            "evidence_completeness": report.get("evidence_completeness").cloned().unwrap_or(Value::Null),
            "report_sha256": report_digest.clone(),
        },
        "report_excerpt": {
            "schema_version": report.get("schema_version").cloned().unwrap_or(Value::Null),
            "repo": report.get("repo").cloned().unwrap_or_else(|| json!({})),
            "summary": report.get("summary").cloned().unwrap_or_else(|| json!({})),
            "stats": report.get("stats").cloned().unwrap_or_else(|| json!({})),
            "analyzer": report.get("analyzer").cloned().unwrap_or_else(|| json!({})),
            "evidence_excerpts": evidence_excerpts,
            "collection_metadata": report.get("collection_metadata").cloned().unwrap_or(Value::Null),
            "truncation": {"evidence_excerpt_limit": 20, "evidence_excerpt_total": report.get("files").and_then(Value::as_array).map(Vec::len).unwrap_or_default()},
        },
        "repository_context": repository_context,
        "boundary": "Local model output is advisory only and must not rescore detector truth or mutate code, GitHub, or report data.",
    });
    let context_json = render_json(&context)?;
    let prompt = format!(
        "# git-slop {command} prompt pack\n\n\
             Use only the facts in context.json.\n\n\
             Summarize the selected detector evidence for a maintainer. Keep hotspot\n\
             costs separate from overlay evidence. Treat model output as advisory:\n\
             do not rescore detector truth, claim correctness, or imply a refactor is\n\
             mandatory. If the supplied facts are insufficient, say what is missing\n\
             instead of inventing context.\n\n\
             Preferred output:\n\n\
             1. Brief maintainer summary\n\
             2. Strongest evidence\n\
             3. Suggested next review step\n\
             4. Boundaries and non-claims\n"
    );
    let readme = format!(
        "# git-slop {command} Prompt Pack\n\n\
             This directory was generated by git-slop for local model use.\n\n\
             Files:\n\n\
             - `context.json`: deterministic git-slop payload plus minimal report metadata.\n\
             - `prompt.md`: prompt text for a local model.\n\
             - `README.md`: these usage notes.\n\
             - `response-template.md`: evidence-aware answer structure.\n\
             - `verification.md`: reproducibility checks.\n\n\
             Boundary rules:\n\n\
             - This pack is advisory only.\n\
             - Local model output must not mutate code, GitHub, or detector truth.\n\
             - Local model output must not rescore detector truth, including\n\
               `slop_score`, `slop_band`,\n\
               `context_band`, or `git slop check` semantics.\n\
             - Keep hotspot cost separate from overlay evidence.\n\
             - `manifest.json` binds every payload file to its SHA-256 digest.\n"
    );
    let response_template = "# Maintainer response\n\n## Summary\n\n## Evidence cited\n\n- Path, metric, and content fingerprint:\n\n## Proposed next step\n\n## Verification\n\n## Missing evidence and non-claims\n".to_string();
    let verification = format!(
        "# Verification\n\n- Validate the source report: `git-slop report validate --allow-legacy <report.json>`\n- Reproduce the selected payload with `git-slop {command} --format json` and the selector in context.json.\n- Confirm context.json and prompt.md digests against manifest.json.\n- Do not treat model prose as detector evidence.\n"
    );
    let files = [
        ("context.json", context_json),
        ("prompt.md", prompt),
        ("README.md", readme),
        ("response-template.md", response_template),
        ("verification.md", verification),
    ];
    for (name, contents) in &files {
        fs::write(temporary.join(name), contents)?;
    }
    let digests = files
        .iter()
        .map(|(name, contents)| {
            (
                name.to_string(),
                hex::encode(Sha256::digest(contents.as_bytes())),
            )
        })
        .collect::<BTreeMap<_, _>>();
    let manifest = json!({
        "schema_version": 1,
        "prompt_pack_version": 1,
        "command": command,
        "files": digests,
        "report": {
            "schema_version": report.get("schema_version").cloned().unwrap_or(Value::Null),
            "config_digest": report.pointer("/analyzer/config_digest").cloned().unwrap_or(Value::Null),
            "scope": report.get("scope").cloned().unwrap_or(Value::Null),
            "head_sha": report.pointer("/repo/head_sha").cloned().unwrap_or(Value::Null),
            "report_sha256": report_digest,
            "generated_at": report.get("generated_at").cloned().unwrap_or(Value::Null)
        }
    });
    fs::write(temporary.join("manifest.json"), render_json(&manifest)?)?;
    let backup = parent.join(format!(".{name}-{}-backup", std::process::id()));
    let had_existing = output_dir.exists();
    if had_existing {
        fs::rename(output_dir, &backup)?;
    }
    if let Err(error) = fs::rename(&temporary, output_dir) {
        let _ = fs::remove_dir_all(&temporary);
        if had_existing {
            let _ = fs::rename(&backup, output_dir);
        }
        return Err(error.into());
    }
    if had_existing {
        fs::remove_dir_all(backup)?;
    }
    Ok(())
}
