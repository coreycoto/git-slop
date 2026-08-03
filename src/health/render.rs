use anyhow::Result;
use percent_encoding::{AsciiSet, CONTROLS, utf8_percent_encode};
use serde_json::{Value, json};

use super::model::*;
use super::rollup::health_rollup_from_report;
use crate::model::{Analysis, HealthRollup};

const URL_PATH_SEGMENT_ENCODE_SET: &AsciiSet = &CONTROLS
    .add(b' ')
    .add(b'"')
    .add(b'#')
    .add(b'%')
    .add(b'/')
    .add(b'<')
    .add(b'>')
    .add(b'?')
    .add(b'[')
    .add(b'\\')
    .add(b']')
    .add(b'^')
    .add(b'`')
    .add(b'{')
    .add(b'|')
    .add(b'}');

fn visible_controls(value: &str) -> String {
    let mut rendered = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '\n' => rendered.push_str("\\n"),
            '\r' => rendered.push_str("\\r"),
            '\t' => rendered.push_str("\\t"),
            control if control.is_control() => {
                rendered.push_str(&format!("\\u{{{:x}}}", control as u32));
            }
            printable => rendered.push(printable),
        }
    }
    rendered
}

fn markdown_escape(value: &str) -> String {
    visible_controls(value)
        .replace('\\', "\\\\")
        .replace('|', "\\|")
        .replace('[', "\\[")
        .replace(']', "\\]")
        .replace('<', "\\<")
        .replace('>', "\\>")
        .replace('`', "\\`")
}

fn inline_code(value: &str) -> String {
    let value = visible_controls(value);
    let mut longest_run: usize = 0;
    let mut current_run: usize = 0;
    for character in value.chars() {
        if character == '`' {
            current_run += 1;
            longest_run = longest_run.max(current_run);
        } else {
            current_run = 0;
        }
    }
    if longest_run == 0 {
        return format!("`{value}`");
    }
    let fence = "`".repeat(longest_run.saturating_add(1));
    format!("{fence} {value} {fence}")
}

fn format_int(value: usize) -> String {
    let digits = value.to_string();
    let mut rendered = String::with_capacity(digits.len() + digits.len() / 3);
    for (index, character) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index) % 3 == 0 {
            rendered.push(',');
        }
        rendered.push(character);
    }
    rendered
}

fn format_number(value: f64) -> String {
    if (value - value.round()).abs() < 0.005 {
        format_int(value.round().max(0.0) as usize)
    } else {
        format!("{value:.2}")
    }
}

fn format_percent(value: f64) -> String {
    format!("{:.1}%", value * 100.0)
}

fn repository_slug(remote: Option<&str>) -> Option<String> {
    let remote = remote?.trim().trim_end_matches('/');
    let candidate = if let Some(rest) = remote.strip_prefix("git@github.com:") {
        rest
    } else if let Some(rest) = remote.strip_prefix("ssh://git@github.com/") {
        rest
    } else if let Some(rest) = remote.strip_prefix("https://github.com/") {
        rest
    } else {
        remote.strip_prefix("http://github.com/")?
    };
    let slug = candidate.trim_end_matches(".git").trim_matches('/');
    (slug.split('/').count() == 2).then(|| slug.to_string())
}

fn encoded_repo_path(path: &str) -> String {
    path.split('/')
        .map(|segment| utf8_percent_encode(segment, URL_PATH_SEGMENT_ENCODE_SET).to_string())
        .collect::<Vec<_>>()
        .join("/")
}

pub fn github_blob_url(report: &Value, path: &str) -> Option<String> {
    let repo = report.get("repo")?;
    let remote = repo
        .get("git_remote_url")
        .and_then(Value::as_str)
        .or_else(|| repo.get("remote_url").and_then(Value::as_str));
    let slug = repository_slug(remote)?;
    let revision = repo
        .get("head_commit")
        .and_then(Value::as_str)
        .or_else(|| repo.get("head_sha").and_then(Value::as_str))?;
    Some(format!(
        "https://github.com/{slug}/blob/{revision}/{}",
        encoded_repo_path(path)
    ))
}

fn path_cell(report: &Value, path: &str) -> String {
    if let Some(url) = github_blob_url(report, path) {
        format!("[{}]({url})", markdown_escape(path))
    } else {
        inline_code(path)
    }
}

fn repo_label(report: &Value) -> String {
    let repo = report.get("repo").unwrap_or(&Value::Null);
    let remote = repo
        .get("git_remote_url")
        .and_then(Value::as_str)
        .or_else(|| repo.get("remote_url").and_then(Value::as_str));
    repository_slug(remote)
        .or_else(|| {
            repo.get("repo_name")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned)
        })
        .unwrap_or_else(|| "unknown".to_string())
}

fn distribution_number(distribution: &Value, key: &str) -> f64 {
    distribution
        .get(key)
        .and_then(Value::as_f64)
        .or_else(|| {
            distribution
                .get(key)
                .and_then(Value::as_u64)
                .map(|number| number as f64)
        })
        .unwrap_or_default()
}

fn candidate_parent_share(report: &Value, candidate: &Value) -> Option<f64> {
    let path = string_field(candidate, "path").trim_end_matches('/');
    let parent = path
        .rsplit_once('/')
        .map(|(parent, _)| parent)
        .unwrap_or(".");
    let parent_tokens = candidate
        .get("parent_tokens")
        .and_then(Value::as_u64)
        .and_then(|tokens| usize::try_from(tokens).ok())
        .unwrap_or_else(|| {
            report
                .get("folders")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .find(|folder| string_field(folder, "path").trim_end_matches('/') == parent)
                .map(|folder| usize_field(folder, "tokens"))
                .unwrap_or_default()
        });
    if parent_tokens == 0 {
        None
    } else {
        Some(usize_field(candidate, "tokens") as f64 / parent_tokens as f64)
    }
}

fn render_candidate_tables(
    lines: &mut Vec<String>,
    report: &Value,
    candidates: &[Value],
    file_limit: usize,
    folder_limit: usize,
) {
    let files = candidates
        .iter()
        .filter(|candidate| string_field(candidate, "kind") == "file")
        .take(file_limit)
        .collect::<Vec<_>>();
    if !files.is_empty() {
        lines.extend([
            "#### File Risks".to_string(),
            String::new(),
            "| Path | Class | Tokens | Band | % of parent |".to_string(),
            "| --- | --- | ---: | --- | ---: |".to_string(),
        ]);
        for item in files {
            let share = candidate_parent_share(report, item)
                .map(format_percent)
                .unwrap_or_else(|| "-".to_string());
            lines.push(format!(
                "| {} | {} | {} | {} | {} |",
                path_cell(report, string_field(item, "path")),
                markdown_escape(string_field(item, "class")),
                format_int(usize_field(item, "tokens")),
                inline_code(string_field(item, "band")),
                share
            ));
        }
        lines.push(String::new());
    }
    let folders = candidates
        .iter()
        .filter(|candidate| string_field(candidate, "kind") == "folder")
        .take(folder_limit)
        .collect::<Vec<_>>();
    if !folders.is_empty() {
        lines.extend([
            "#### Folder Risks".to_string(),
            String::new(),
            "| Path | Class | Direct files | Direct tokens | Band | % of parent |".to_string(),
            "| --- | --- | ---: | ---: | --- | ---: |".to_string(),
        ]);
        for item in folders {
            let share = candidate_parent_share(report, item)
                .map(format_percent)
                .unwrap_or_else(|| "-".to_string());
            lines.push(format!(
                "| {} | {} | {} | {} | {} | {} |",
                path_cell(report, string_field(item, "path")),
                markdown_escape(string_field(item, "class")),
                format_int(usize_field(item, "files")),
                format_int(usize_field(item, "tokens")),
                inline_code(string_field(item, "band")),
                share
            ));
        }
        lines.push(String::new());
    }
}

fn render_health_value(report: &Value, rollup: &HealthRollup) -> String {
    let config = report.get("config").unwrap_or(&Value::Null);
    let compact_max = config_u64(
        config,
        "/tokenization/context_bands/compact_max_tokens",
        DEFAULT_COMPACT_MAX,
    );
    let healthy_max = config_u64(
        config,
        "/tokenization/context_bands/healthy_max_tokens",
        DEFAULT_HEALTHY_MAX,
    );
    let warning_max = config_u64(
        config,
        "/tokenization/context_bands/warning_max_tokens",
        DEFAULT_WARNING_MAX,
    );
    let folder_compact_max = config_u64(
        config,
        "/health/folder_bands/compact_max_direct_tokens",
        DEFAULT_FOLDER_COMPACT_MAX,
    );
    let folder_healthy_max = config_u64(
        config,
        "/health/folder_bands/healthy_max_direct_tokens",
        DEFAULT_FOLDER_HEALTHY_MAX,
    );
    let folder_warning_max = config_u64(
        config,
        "/health/folder_bands/warning_max_direct_tokens",
        DEFAULT_FOLDER_WARNING_MAX,
    );
    let folder_warning_files = config_u64(
        config,
        "/health/folder_bands/warning_max_direct_files",
        DEFAULT_FOLDER_WARNING_FILES,
    );
    let folder_refactor_files = config_u64(
        config,
        "/health/folder_bands/refactor_required_max_direct_files",
        DEFAULT_FOLDER_REFACTOR_FILES,
    );
    let top_files = config_usize(config, "/health/summary_top_files", DEFAULT_TOP_FILES);
    let top_folders = config_usize(config, "/health/summary_top_folders", DEFAULT_TOP_FOLDERS);
    let repo = report.get("repo").unwrap_or(&Value::Null);
    let generated_at = report
        .get("generated_at")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    let analyzed_revision_at = report
        .get("analyzed_revision_at")
        .and_then(Value::as_str)
        .or_else(|| repo.get("head_commit_timestamp").and_then(Value::as_str));
    let branch = repo
        .get("branch")
        .and_then(Value::as_str)
        .unwrap_or("detached");
    let head = repo
        .get("head_commit")
        .and_then(Value::as_str)
        .or_else(|| repo.get("head_sha").and_then(Value::as_str))
        .unwrap_or("none");
    let file_failures = *rollup
        .file_band_counts
        .get("refactor_required")
        .unwrap_or(&0);
    let folder_failures = *rollup
        .folder_band_counts
        .get("refactor_required")
        .unwrap_or(&0);
    let file_warnings = *rollup.file_band_counts.get("warning").unwrap_or(&0);
    let folder_warnings = *rollup.folder_band_counts.get("warning").unwrap_or(&0);
    let status = if file_failures + folder_failures > 0 {
        format!(
            "❌ **Review required** — {} file(s) and {} folder(s) exceed configured refactor thresholds.",
            file_failures, folder_failures
        )
    } else if file_warnings + folder_warnings > 0 {
        format!(
            "⚠️ **Advisory** — {} file(s) and {} folder(s) are in warning bands.",
            file_warnings, folder_warnings
        )
    } else {
        "✅ **Within configured context budgets.**".to_string()
    };
    let mut lines = vec![
        "# Repository Health".to_string(),
        String::new(),
        status,
        String::new(),
        format!("- **Generated at:** {}", inline_code(generated_at)),
        format!("- **Repo:** {}", inline_code(&repo_label(report))),
        format!("- **Branch:** {}", inline_code(branch)),
        format!("- **Head SHA:** {}", inline_code(head)),
    ];
    if let Some(timestamp) = analyzed_revision_at {
        lines.push(format!(
            "- **Analyzed revision timestamp:** {}",
            inline_code(timestamp)
        ));
    }
    if repo
        .get("is_shallow")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        lines.push(
            "- **History:** ⚠️ shallow checkout; history-based evidence may be incomplete."
                .to_string(),
        );
    }

    lines.extend([
        String::new(),
        "## Summary".to_string(),
        String::new(),
        "### `agent_context`".to_string(),
        String::new(),
        "#### Status".to_string(),
        String::new(),
        "| Band | Definition | Files |".to_string(),
        "| --- | --- | ---: |".to_string(),
        format!(
            "| `compact` | `<= {}` tokens | {} |",
            format_int(compact_max as usize),
            rollup.file_band_counts.get("compact").unwrap_or(&0)
        ),
        format!(
            "| `healthy` | `{}-{}` tokens | {} |",
            format_int(compact_max.saturating_add(1) as usize),
            format_int(healthy_max as usize),
            rollup.file_band_counts.get("healthy").unwrap_or(&0)
        ),
        format!(
            "| `warning` | `{}-{}` tokens | {} |",
            format_int(healthy_max.saturating_add(1) as usize),
            format_int(warning_max as usize),
            rollup.file_band_counts.get("warning").unwrap_or(&0)
        ),
        format!(
            "| `refactor_required` | `>{}` tokens | {} |",
            format_int(warning_max as usize),
            rollup
                .file_band_counts
                .get("refactor_required")
                .unwrap_or(&0)
        ),
        String::new(),
        "| Band | Definition | Folders |".to_string(),
        "| --- | --- | ---: |".to_string(),
        format!(
            "| `compact` | direct tokens `<= {}` | {} |",
            format_int(folder_compact_max as usize),
            rollup.folder_band_counts.get("compact").unwrap_or(&0)
        ),
        format!(
            "| `healthy` | direct tokens `{}-{}` | {} |",
            format_int(folder_compact_max.saturating_add(1) as usize),
            format_int(folder_healthy_max as usize),
            rollup.folder_band_counts.get("healthy").unwrap_or(&0)
        ),
        format!(
            "| `warning` | direct tokens `{}-{}` or direct files `>{}` | {} |",
            format_int(folder_healthy_max.saturating_add(1) as usize),
            format_int(folder_warning_max as usize),
            folder_warning_files,
            rollup.folder_band_counts.get("warning").unwrap_or(&0)
        ),
        format!(
            "| `refactor_required` | direct tokens `>{}` or direct files `>{}` | {} |",
            format_int(folder_warning_max as usize),
            folder_refactor_files,
            rollup
                .folder_band_counts
                .get("refactor_required")
                .unwrap_or(&0)
        ),
        String::new(),
        "#### Token Stats".to_string(),
        String::new(),
        "| Type | p50 | p90 | p95 | p99 | Max | Top 1 share | Top 5 share | Top 10 share |"
            .to_string(),
        "| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |".to_string(),
    ]);
    for (label, stats) in [
        ("files", &rollup.file_distribution),
        ("folders", &rollup.folder_distribution),
    ] {
        lines.push(format!(
            "| `{label}` | {} | {} | {} | {} | {} | {} | {} | {} |",
            format_number(distribution_number(stats, "p50")),
            format_number(distribution_number(stats, "p90")),
            format_number(distribution_number(stats, "p95")),
            format_number(distribution_number(stats, "p99")),
            format_number(distribution_number(stats, "max")),
            format_percent(distribution_number(stats, "top_1_share")),
            format_percent(distribution_number(stats, "top_5_share")),
            format_percent(distribution_number(stats, "top_10_share")),
        ));
    }

    let must_refactor = rollup
        .refactor_candidates
        .iter()
        .filter(|candidate| string_field(candidate, "band") == "refactor_required")
        .cloned()
        .collect::<Vec<_>>();
    let should_refactor = rollup
        .refactor_candidates
        .iter()
        .filter(|candidate| string_field(candidate, "band") == "warning")
        .cloned()
        .collect::<Vec<_>>();
    if !must_refactor.is_empty() || !should_refactor.is_empty() || !rollup.watchlist.is_empty() {
        lines.extend([String::new(), "## Refactor Recommendations".to_string()]);
    }
    if !must_refactor.is_empty() {
        lines.extend([
            String::new(),
            "### Policy Failures".to_string(),
            String::new(),
        ]);
        render_candidate_tables(&mut lines, report, &must_refactor, top_files, top_folders);
    }
    if !should_refactor.is_empty() {
        lines.extend([
            String::new(),
            "### Review Candidates".to_string(),
            String::new(),
        ]);
        render_candidate_tables(&mut lines, report, &should_refactor, top_files, top_folders);
    }
    if !rollup.watchlist.is_empty() {
        lines.extend([String::new(), "### Watchlist".to_string(), String::new()]);
        render_candidate_tables(
            &mut lines,
            report,
            &rollup.watchlist,
            top_files,
            top_folders,
        );
    }

    if !rollup.findings.is_empty() {
        lines.extend([
            String::new(),
            "## Actionable Findings".to_string(),
            String::new(),
            "| Severity | Path | Why it surfaced | Next step |".to_string(),
            "| --- | --- | --- | --- |".to_string(),
        ]);
        for finding in rollup.findings.iter().take(top_files.max(1)) {
            lines.push(format!(
                "| {} | {} | {} | {} |",
                inline_code(&finding.severity),
                path_cell(report, &finding.path),
                markdown_escape(&finding.reasons.join("; ")),
                inline_code(&finding.next_command),
            ));
        }
    }

    if !rollup.profile_rollups.is_empty() || !rollup.language_rollups.is_empty() {
        lines.extend([
            String::new(),
            "## Rollups".to_string(),
            String::new(),
            "<details>".to_string(),
            "<summary>By profile and language</summary>".to_string(),
            String::new(),
        ]);
        if !rollup.profile_rollups.is_empty() {
            lines.extend([
                "### By Profile".to_string(),
                String::new(),
                "| Profile | Files | Lines | Code | Comments | Blanks | Tokens |".to_string(),
                "| --- | ---: | ---: | ---: | ---: | ---: | ---: |".to_string(),
            ]);
            for profile in &rollup.profile_rollups {
                let totals = profile.get("totals").unwrap_or(&Value::Null);
                lines.push(format!(
                    "| {} | {} | {} | {} | {} | {} | {} |",
                    inline_code(string_field(profile, "name")),
                    format_int(usize_field(totals, "files")),
                    format_int(usize_field(totals, "lines")),
                    format_int(usize_field(totals, "code")),
                    format_int(usize_field(totals, "comments")),
                    format_int(usize_field(totals, "blanks")),
                    format_int(usize_field(totals, "tokens")),
                ));
            }
        }
        for (profile, languages) in &rollup.language_rollups {
            if languages.is_empty() {
                continue;
            }
            lines.extend([
                String::new(),
                format!("### By Language · {}", inline_code(profile)),
                String::new(),
                "| Language | Files | Lines | Tokens | % of profile tokens |".to_string(),
                "| --- | ---: | ---: | ---: | ---: |".to_string(),
            ]);
            for language in languages {
                lines.push(format!(
                    "| {} | {} | {} | {} | {} |",
                    markdown_escape(string_field(language, "language")),
                    format_int(usize_field(language, "files")),
                    format_int(usize_field(language, "lines")),
                    format_int(usize_field(language, "tokens")),
                    format_percent(float_field(language, "token_share")),
                ));
            }
        }
        lines.extend([String::new(), "</details>".to_string()]);
    }

    let stats = report.get("stats").unwrap_or(&Value::Null);
    let skipped_total = [
        "skipped_ignored_count",
        "skipped_missing_count",
        "skipped_binary_count",
        "skipped_undecodable_count",
    ]
    .iter()
    .map(|key| usize_field(stats, key))
    .sum::<usize>();
    if skipped_total > 0 {
        lines.extend([
            String::new(),
            "## Notes".to_string(),
            String::new(),
            format!(
                "- Skipped {} tracked path(s): ignored {}, missing {}, binary {}, undecodable {}.",
                skipped_total,
                usize_field(stats, "skipped_ignored_count"),
                usize_field(stats, "skipped_missing_count"),
                usize_field(stats, "skipped_binary_count"),
                usize_field(stats, "skipped_undecodable_count"),
            ),
        ]);
    }
    lines.extend([
        String::new(),
        "> Git Slop reports deterministic context and maintenance-pressure evidence. Findings are not correctness proofs or automatic refactor mandates.".to_string(),
    ]);
    format!("{}\n", lines.join("\n"))
}

pub fn render_health_markdown(analysis: &Analysis, rollup: &HealthRollup) -> String {
    let report = json!({
        "schema_version": 4,
        "generated_at": analysis.generated_at,
        "analyzed_revision_at": analysis.analyzed_revision_at,
        "repo": analysis.repo,
        "config": analysis.config,
        "stats": {
            "tracked_file_count": analysis.tracked_file_count,
            "analyzed_file_count": analysis.files.len(),
            "skipped_ignored_count": analysis.skipped.ignored,
            "skipped_missing_count": analysis.skipped.missing,
            "skipped_binary_count": analysis.skipped.binary,
            "skipped_undecodable_count": analysis.skipped.undecodable
        },
        "files": analysis.files,
        "folders": analysis.folders
    });
    render_health_value(&report, rollup)
}

pub fn render_health_from_report(report: &Value) -> Result<String> {
    let rollup = health_rollup_from_report(report)?;
    Ok(render_health_value(report, &rollup))
}
