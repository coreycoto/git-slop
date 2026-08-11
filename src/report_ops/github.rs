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
                } else if matches!(
                    key.as_str(),
                    "scope_paths" | "in_scope" | "nearby_tests" | "nearby_test_paths"
                ) {
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
    let mut paths = Vec::new();
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
    for path in ["AGENTS.md", "CONTRIBUTING.md", "README.md"] {
        if !paths.iter().any(|existing| existing == path) {
            paths.push(path.to_string());
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
        "sha256": hex::encode(Sha256::digest(&bytes)),
        "line_range": if bytes.is_empty() { Value::Null } else { json!({"start": 1, "end": String::from_utf8_lossy(&bytes).lines().count().max(1)}) },
        "text": String::from_utf8_lossy(&bytes)
    }))
}

fn repository_context(payload: &Value, report: &Value, root: &Path, excerpt_bytes: usize) -> Value {
    let byte_limit = excerpt_bytes.clamp(256, 4096);
    let mut candidate_paths = Vec::new();
    collect_prompt_paths(payload, &mut candidate_paths);
    let source_budget = 131_072usize;
    let mut source_bytes = 0usize;
    let source_excerpts = candidate_paths
        .iter()
        .take(10)
        .filter_map(|path| {
            let absolute = root.join(path);
            let metadata = fs::symlink_metadata(&absolute).ok()?;
            if !metadata.is_file() || metadata.file_type().is_symlink() {
                return None;
            }
            let bytes = fs::read(absolute).ok()?;
            if bytes.len() > source_budget.saturating_sub(source_bytes) {
                return bounded_repository_text(root, path, byte_limit).map(|mut value| {
                    value["included_complete"] = json!(false);
                    value["reason"] = json!("source_budget_exceeded");
                    value
                });
            }
            source_bytes += bytes.len();
            Some(json!({
                "path": path,
                "excerpt": String::from_utf8_lossy(&bytes),
                "bytes_returned": bytes.len(),
                "truncated": false,
                "included_complete": true,
                "sha256": hex::encode(Sha256::digest(&bytes))
            }))
        })
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
    let source_complete = source_excerpts.len() == candidate_paths.len().min(10)
        && source_excerpts
            .iter()
            .all(|item| item.get("included_complete").and_then(Value::as_bool) == Some(true))
        && candidate_paths.len() <= 10;
    let configured_commands = string_array(report.pointer("/config/verification/commands"));
    let verification_commands =
        super::verification::from_worktree(root, &candidate_paths, &configured_commands);
    json!({
        "included": true,
        "planning_usable": guidance_complete,
        "execution_ready": guidance_complete && source_complete && !candidate_paths.is_empty(),
        "execution_usable": guidance_complete && source_complete && !candidate_paths.is_empty(),
        "reason": "explicit_opt_in",
        "source_excerpts": source_excerpts,
        "guidance": guidance,
        "verification_commands": verification_commands,
        "truncation": {
            "per_file_byte_limit": byte_limit,
            "source_file_limit": 10,
            "source_candidate_count": candidate_paths.len(),
            "source_returned_count": source_excerpts.len(),
            "source_total_byte_budget": source_budget,
            "source_bytes_returned": source_bytes,
            "source_complete": source_complete,
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

pub struct PromptPackOptions<'a> {
    pub repository_root: Option<&'a Path>,
    pub excerpt_bytes: usize,
    pub force: bool,
    pub include_local_paths: bool,
}

pub fn write_prompt_pack(
    command: &str,
    payload: &Value,
    report: &Value,
    report_path: &Path,
    output_dir: &Path,
    options: PromptPackOptions<'_>,
) -> Result<()> {
    if output_dir.exists() {
        if !output_dir.is_dir() {
            bail!(
                "Prompt pack path is not a directory: {}",
                output_dir.display()
            );
        }
        let empty = fs::read_dir(output_dir)?.next().is_none();
        if !empty && !options.force {
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
    let source_report_bytes = fs::read(report_path).map_err(|error| {
        anyhow!(
            "failed to read source report {}: {error}",
            report_path.display()
        )
    })?;
    let report_digest = hex::encode(Sha256::digest(&source_report_bytes));
    let canonical_report_digest = serde_json::to_vec(report)
        .map(|bytes| hex::encode(Sha256::digest(bytes)))
        .unwrap_or_default();
    let mut selected_paths = Vec::new();
    collect_prompt_paths(payload, &mut selected_paths);
    let mut report_files = array_at(report, &["files"])
        .iter()
        .filter(|file| {
            file.get("path")
                .and_then(Value::as_str)
                .is_some_and(|path| selected_paths.iter().any(|selected| selected == path))
        })
        .collect::<Vec<_>>();
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
                "content_fingerprint": file.get("content_fingerprint"),
                "content_sha256": file.get("content_sha256")
            })
        })
        .collect::<Vec<_>>();
    let repository_context = options.repository_root.map_or_else(
        || {
            json!({
                "included": false,
                "planning_usable": false,
                "execution_ready": false,
                "execution_usable": false,
                "reason": "not_requested",
                "source_excerpts": [],
                "guidance": [],
                "verification_commands": [],
                "truncation": {"per_file_byte_limit": 0, "file_limit": 0}
            })
        },
        |root| repository_context(payload, report, root, options.excerpt_bytes),
    );
    let readiness = evaluate_report_readiness(report, false, false);
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
            "canonical_report_sha256": canonical_report_digest.clone(),
            "canonicalization": "serde_json_compact_preserve_order_v1",
        },
        "readiness": readiness.as_json(),
        "report_excerpt": {
            "schema_version": report.get("schema_version").cloned().unwrap_or(Value::Null),
            "repo": report.get("repo").cloned().unwrap_or_else(|| json!({})),
            "summary": report.get("summary").cloned().unwrap_or_else(|| json!({})),
            "stats": report.get("stats").cloned().unwrap_or_else(|| json!({})),
            "analyzer": report.get("analyzer").cloned().unwrap_or_else(|| json!({})),
            "evidence_excerpts": evidence_excerpts,
            "collection_metadata": report.get("collection_metadata").cloned().unwrap_or(Value::Null),
            "truncation": {"evidence_excerpt_limit": 20, "evidence_excerpt_candidate_count": selected_paths.len(), "evidence_excerpt_returned": evidence_excerpts.len()},
        },
        "repository_context": repository_context,
        "boundary": "Local model output is advisory only and must not rescore detector truth or mutate code, GitHub, or report data.",
    });
    let context_json = render_json(&context)?;
    let prompt = if command == "plan" {
        "# git-slop plan prompt pack\n\nUse only the facts in context.json. Produce a bounded, ordered implementation proposal tied to the selected paths, detector reason codes, and supplied verification commands. Name concrete files and tests. Cite missing evidence instead of inventing context. Do not invent symbols or evidence, mutate the repository, or claim the proposal is mandatory; do not rescore detector truth. Mark missing evidence explicitly.\n\nPreferred output:\n\n1. Scope and acceptance criteria\n2. Ordered implementation slices\n3. File-specific changes\n4. Verification commands\n5. Risks, rollback, and non-claims\n".to_string()
    } else {
        "# git-slop explain prompt pack\n\nUse only the facts in context.json. Explain the selected detector evidence for a maintainer. Cite missing evidence instead of inventing context. Keep hotspot costs separate from overlay evidence; do not rescore detector truth, claim correctness, or prescribe a refactor as mandatory. Mark missing evidence explicitly.\n\nPreferred output:\n\n1. Brief maintainer summary\n2. Strongest evidence\n3. Plausible interpretations\n4. Suggested next inspection\n5. Boundaries and non-claims\n".to_string()
    };
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
    let response_template = if command == "plan" {
        "# Maintainer plan\n\n## Scope and acceptance criteria\n\n## Ordered slices\n\n## Files and symbols\n\n## Verification commands\n\n## Risks and rollback\n\n## Missing evidence and non-claims\n".to_string()
    } else {
        "# Maintainer explanation\n\n## Summary\n\n## Evidence cited\n\n- Path, metric, raw SHA-256, and semantic fingerprint:\n\n## Interpretation\n\n## Next inspection\n\n## Missing evidence and non-claims\n".to_string()
    };
    let report_descriptor = options
        .include_local_paths
        .then(|| report_path.to_path_buf());
    let report_argument = report_descriptor.as_ref().map_or_else(
        || "'<SOURCE_REPORT>'".to_string(),
        |path| format!("'{}'", path.display().to_string().replace('\'', "'\\''")),
    );
    let selector_kind = payload
        .pointer("/selector/kind")
        .and_then(Value::as_str)
        .unwrap_or("top");
    let selector_value = payload
        .pointer("/selector/value")
        .and_then(Value::as_str)
        .unwrap_or("5");
    let selector_flag = match selector_kind {
        "path" => "--path",
        "cluster" => "--cluster",
        "relationship" => "--relationship",
        _ => "--top",
    };
    let selector_argument = format!("'{}'", selector_value.replace('\'', "'\\''"));
    let verification = format!(
        "# Verification\n\n- Validate the exact source report: `git slop report validate {report_argument}`\n- Reproduce the selected payload: `git slop {command} --report {report_argument} {selector_flag} {selector_argument} --format json`\n- Confirm the source bytes have SHA-256 `{report_digest}`.\n- Confirm context.json and prompt.md digests against manifest.json.\n- Do not treat model prose as detector evidence.\n"
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
            "canonical_report_sha256": canonical_report_digest,
            "canonicalization": "serde_json_compact_preserve_order_v1",
            "source_path": report_descriptor,
            "source_descriptor": if options.include_local_paths { "local_path" } else { "logical_source_report" },
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
