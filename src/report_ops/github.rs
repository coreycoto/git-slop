use super::*;

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

fn github_property_escape(value: &str) -> String {
    value
        .replace('%', "%25")
        .replace('\r', "%0D")
        .replace('\n', "%0A")
        .replace(':', "%3A")
        .replace(',', "%2C")
}

fn github_message_escape(value: &str) -> String {
    value
        .replace('%', "%25")
        .replace('\r', "%0D")
        .replace('\n', "%0A")
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
) -> Result<()> {
    if output_dir.exists() {
        if !output_dir.is_dir() {
            bail!(
                "Prompt pack path is not a directory: {}",
                output_dir.display()
            );
        }
        bail!(
            "Prompt pack path already exists; choose a new directory: {}",
            output_dir.display()
        );
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
    let context = json!({
        "prompt_pack_version": 1,
        "command": command,
        "payload": payload,
        "report_excerpt": {
            "schema_version": report.get("schema_version").cloned().unwrap_or(Value::Null),
            "repo": report.get("repo").cloned().unwrap_or_else(|| json!({})),
            "summary": report.get("summary").cloned().unwrap_or_else(|| json!({})),
            "stats": report.get("stats").cloned().unwrap_or_else(|| json!({})),
        },
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
             - `README.md`: these usage notes.\n\n\
             Boundary rules:\n\n\
             - This pack is advisory only.\n\
             - Local model output must not mutate code, GitHub, or detector truth.\n\
             - Local model output must not rescore detector truth, including\n\
               `slop_score`, `slop_band`,\n\
               `context_band`, or `git slop check` semantics.\n\
             - Keep hotspot cost separate from overlay evidence.\n\
             - `manifest.json` binds every payload file to its SHA-256 digest.\n"
    );
    let files = [
        ("context.json", context_json),
        ("prompt.md", prompt),
        ("README.md", readme),
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
            "scope": report.get("scope").cloned().unwrap_or(Value::Null)
        }
    });
    fs::write(temporary.join("manifest.json"), render_json(&manifest)?)?;
    fs::rename(&temporary, output_dir).inspect_err(|_| {
        let _ = fs::remove_dir_all(&temporary);
    })?;
    Ok(())
}
