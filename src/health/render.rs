use anyhow::Result;
use percent_encoding::{AsciiSet, CONTROLS, utf8_percent_encode};
use serde_json::Value;

use super::model::*;
use super::rollup::health_rollup_from_report;
use crate::model::HealthRollup;
use crate::text::{inline_code, markdown_escape, visible_controls};

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

pub(super) fn format_number(value: f64) -> String {
    let rounded = ((value.max(0.0) * 100.0).round()) / 100.0;
    if rounded.fract().abs() < f64::EPSILON {
        return format_int(rounded as usize);
    }
    let fixed = format!("{rounded:.2}");
    let (whole, fraction) = fixed.split_once('.').unwrap_or((&fixed, "00"));
    let whole = whole.parse::<usize>().unwrap_or_default();
    format!("{}.{fraction}", format_int(whole))
}

pub(super) fn format_percent(value: f64) -> String {
    format!("{:.1}%", value * 100.0)
}

pub(super) fn format_score(value: f64) -> String {
    let fixed = format!("{:.1}", value.max(0.0));
    let (whole, fraction) = fixed.split_once('.').unwrap_or((&fixed, "0"));
    let whole = whole.parse::<usize>().unwrap_or_default();
    format!("{}.{fraction}", format_int(whole))
}

pub(super) fn format_finding_reason(reason: &str, tokens: usize) -> String {
    reason.replacen(
        &format!("{tokens} tokens"),
        &format!("{} tokens", format_int(tokens)),
        1,
    )
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

#[derive(Clone, Copy)]
struct FolderBoundaries {
    healthy_tokens: usize,
    warning_tokens: usize,
    warning_files: usize,
    refactor_files: usize,
}

fn maintenance_pressure(candidate: &Value) -> String {
    let band = string_field(candidate, "slop_band");
    if band.is_empty() {
        "-".to_string()
    } else {
        format!(
            "{} · score {}",
            inline_code(band),
            format_score(float_field(candidate, "slop_score"))
        )
    }
}

fn folder_trigger(candidate: &Value, boundaries: FolderBoundaries) -> String {
    let band = string_field(candidate, "band");
    let files = usize_field(candidate, "files");
    let tokens = usize_field(candidate, "tokens");
    let (token_ceiling, file_ceiling, boundary_label) = match band {
        "budget_exceeded" | "refactor_required" => (
            boundaries.warning_tokens,
            boundaries.refactor_files,
            "warning",
        ),
        "warning" => (
            boundaries.healthy_tokens,
            boundaries.warning_files,
            "healthy",
        ),
        _ => {
            return format!(
                "no warning trigger: {} direct files <= configured healthy ceiling of {}; {} direct tokens <= configured healthy ceiling of {}",
                format_int(files),
                format_int(boundaries.warning_files),
                format_int(tokens),
                format_int(boundaries.healthy_tokens),
            );
        }
    };
    let file_trigger = (files > file_ceiling).then(|| {
        format!(
            "{} direct files > {} {boundary_label} ceiling",
            format_int(files),
            format_int(file_ceiling)
        )
    });
    let token_trigger = (tokens > token_ceiling).then(|| {
        format!(
            "{} direct tokens > {} {boundary_label} ceiling",
            format_int(tokens),
            format_int(token_ceiling)
        )
    });
    match (file_trigger, token_trigger) {
        (Some(files), Some(tokens)) => format!("both: {files}; {tokens}"),
        (Some(files), None) => format!("files: {files}"),
        (None, Some(tokens)) => format!("tokens: {tokens}"),
        (None, None) => format!(
            "no {band} trigger in current direct load: {} files, {} tokens",
            format_int(files),
            format_int(tokens)
        ),
    }
}

fn folder_contains_file(folder: &str, path: &str) -> bool {
    let folder = folder.trim_matches('/');
    folder.is_empty()
        || folder == "."
        || path
            .strip_prefix(folder)
            .is_some_and(|suffix| suffix.starts_with('/'))
}

fn highest_ranked_descendant<'a>(report: &'a Value, folder: &str) -> Option<&'a Value> {
    let mut files = report
        .get("files")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|file| file_profile(file) == "agent_context")
        .filter(|file| folder_contains_file(folder, string_field(file, "path")))
        .collect::<Vec<_>>();
    files.sort_by(|left, right| {
        float_field(right, "slop_score")
            .total_cmp(&float_field(left, "slop_score"))
            .then_with(|| usize_field(right, "tokens").cmp(&usize_field(left, "tokens")))
            .then_with(|| string_field(left, "path").cmp(string_field(right, "path")))
    });
    files.into_iter().next()
}

fn descendant_evidence(report: &Value, folder: &str) -> String {
    let Some(file) = highest_ranked_descendant(report, folder) else {
        return "-".to_string();
    };
    format!(
        "{} — maintenance {} · score {}; context/load {} · {} tokens",
        path_cell(report, string_field(file, "path")),
        inline_code(string_field(file, "slop_band")),
        format_score(float_field(file, "slop_score")),
        inline_code(string_field(file, "context_band")),
        format_int(usize_field(file, "tokens")),
    )
}

fn shell_quote(value: &str) -> String {
    if !value.is_empty()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"/._-".contains(&byte))
    {
        value.to_string()
    } else {
        format!("'{}'", value.replace('\'', "'\"'\"'"))
    }
}

pub(super) fn folder_next_command(path: &str) -> String {
    let normalized = path.trim_matches('/');
    let selector = if normalized.is_empty() || normalized == "." {
        ".".to_string()
    } else {
        format!("{normalized}/")
    };
    format!(
        "git slop explain --path {}",
        shell_quote(&visible_controls(&selector))
    )
}

fn render_candidate_tables(
    lines: &mut Vec<String>,
    report: &Value,
    candidates: &[Value],
    file_limit: usize,
    folder_limit: usize,
    folder_boundaries: FolderBoundaries,
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
            "| Path | Class | Tokens | Context/load band | Maintenance pressure | % of parent |"
                .to_string(),
            "| --- | --- | ---: | --- | --- | ---: |".to_string(),
        ]);
        for item in files {
            let share = candidate_parent_share(report, item)
                .map(format_percent)
                .unwrap_or_else(|| "-".to_string());
            lines.push(format!(
                "| {} | {} | {} | {} | {} | {} |",
                path_cell(report, string_field(item, "path")),
                markdown_escape(string_field(item, "class")),
                format_int(usize_field(item, "tokens")),
                inline_code(string_field(item, "band")),
                maintenance_pressure(item),
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
            "| Path | Class | Direct load | Direct-load band and trigger | Maintenance pressure | Highest-ranked descendant | Next step |"
                .to_string(),
            "| --- | --- | --- | --- | --- | --- | --- |".to_string(),
        ]);
        for item in folders {
            let share = candidate_parent_share(report, item)
                .map(format_percent)
                .unwrap_or_else(|| "-".to_string());
            let path = string_field(item, "path");
            lines.push(format!(
                "| {} | {} | {} files · {} tokens · {} of parent | {} — {} | {} | {} | {} |",
                path_cell(report, path),
                markdown_escape(string_field(item, "class")),
                format_int(usize_field(item, "files")),
                format_int(usize_field(item, "tokens")),
                share,
                inline_code(string_field(item, "band")),
                markdown_escape(&folder_trigger(item, folder_boundaries)),
                maintenance_pressure(item),
                descendant_evidence(report, path),
                inline_code(&folder_next_command(path)),
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
    let folder_boundaries = FolderBoundaries {
        healthy_tokens: folder_healthy_max as usize,
        warning_tokens: folder_warning_max as usize,
        warning_files: folder_warning_files as usize,
        refactor_files: folder_refactor_files as usize,
    };
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
    let file_failures = *rollup.file_band_counts.get("budget_exceeded").unwrap_or(&0);
    let folder_failures = *rollup
        .folder_band_counts
        .get("budget_exceeded")
        .unwrap_or(&0);
    let file_warnings = *rollup.file_band_counts.get("warning").unwrap_or(&0);
    let folder_warnings = *rollup.folder_band_counts.get("warning").unwrap_or(&0);
    let actionable_file_failures = rollup
        .findings
        .iter()
        .filter(|finding| finding.severity == "error")
        .count();
    let observation_only_file_failures = file_failures.saturating_sub(actionable_file_failures);
    let status = if actionable_file_failures > 0 {
        format!(
            "❌ **Review required** — {} actionable file(s) exceed configured context budgets; {} derived/classified file(s) and {} folder(s) remain investigation context.",
            format_int(actionable_file_failures),
            format_int(observation_only_file_failures),
            format_int(folder_failures),
        )
    } else if file_failures + folder_failures > 0 {
        format!(
            "⚠️ **Advisory** — no actionable file breach was found; {} derived/classified file(s) and {} folder(s) exceed context budgets as investigation context.",
            format_int(observation_only_file_failures),
            format_int(folder_failures),
        )
    } else if file_warnings + folder_warnings > 0 {
        format!(
            "⚠️ **Advisory** — {} file(s) and {} folder(s) are in warning bands.",
            format_int(file_warnings),
            format_int(folder_warnings)
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
        "> **Reading the signals:** Context/load bands measure deterministic size against configured context budgets. Maintenance pressure and review severity are separate deterministic review signals; neither is a correctness claim or an automatic refactor mandate.".to_string(),
        String::new(),
        "#### Context/load status".to_string(),
        String::new(),
        "| Context/load band | Definition | Files |".to_string(),
        "| --- | --- | ---: |".to_string(),
        format!(
            "| `compact` | `<= {}` tokens | {} |",
            format_int(compact_max as usize),
            format_int(*rollup.file_band_counts.get("compact").unwrap_or(&0))
        ),
        format!(
            "| `healthy` | `{}-{}` tokens | {} |",
            format_int(compact_max.saturating_add(1) as usize),
            format_int(healthy_max as usize),
            format_int(*rollup.file_band_counts.get("healthy").unwrap_or(&0))
        ),
        format!(
            "| `warning` | `{}-{}` tokens | {} |",
            format_int(healthy_max.saturating_add(1) as usize),
            format_int(warning_max as usize),
            format_int(*rollup.file_band_counts.get("warning").unwrap_or(&0))
        ),
        format!(
            "| `budget_exceeded` | `>{}` tokens | {} |",
            format_int(warning_max as usize),
            format_int(
                *rollup
                    .file_band_counts
                    .get("budget_exceeded")
                    .unwrap_or(&0)
            )
        ),
        String::new(),
        "| Direct-load band | Definition | Folders |".to_string(),
        "| --- | --- | ---: |".to_string(),
        format!(
            "| `compact` | direct tokens `<= {}` | {} |",
            format_int(folder_compact_max as usize),
            format_int(*rollup.folder_band_counts.get("compact").unwrap_or(&0))
        ),
        format!(
            "| `healthy` | direct tokens `{}-{}` | {} |",
            format_int(folder_compact_max.saturating_add(1) as usize),
            format_int(folder_healthy_max as usize),
            format_int(*rollup.folder_band_counts.get("healthy").unwrap_or(&0))
        ),
        format!(
            "| `warning` | direct tokens `{}-{}` or direct files `>{}` | {} |",
            format_int(folder_healthy_max.saturating_add(1) as usize),
            format_int(folder_warning_max as usize),
            format_int(folder_warning_files as usize),
            format_int(*rollup.folder_band_counts.get("warning").unwrap_or(&0))
        ),
        format!(
            "| `budget_exceeded` | direct tokens `>{}` or direct files `>{}` | {} |",
            format_int(folder_warning_max as usize),
            format_int(folder_refactor_files as usize),
            format_int(
                *rollup
                    .folder_band_counts
                    .get("budget_exceeded")
                    .unwrap_or(&0)
            )
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
        .filter(|candidate| string_field(candidate, "band") == "budget_exceeded")
        .cloned()
        .collect::<Vec<_>>();
    let should_refactor = rollup
        .refactor_candidates
        .iter()
        .filter(|candidate| string_field(candidate, "band") == "warning")
        .cloned()
        .collect::<Vec<_>>();
    if !must_refactor.is_empty() || !should_refactor.is_empty() || !rollup.watchlist.is_empty() {
        lines.extend([String::new(), "## Investigation Candidates".to_string()]);
    }
    if !must_refactor.is_empty() {
        lines.extend([
            String::new(),
            "### Context Budget Exceeded".to_string(),
            String::new(),
        ]);
        render_candidate_tables(
            &mut lines,
            report,
            &must_refactor,
            top_files,
            top_folders,
            folder_boundaries,
        );
    }
    if !should_refactor.is_empty() {
        lines.extend([
            String::new(),
            "### Review Candidates".to_string(),
            String::new(),
        ]);
        render_candidate_tables(
            &mut lines,
            report,
            &should_refactor,
            top_files,
            top_folders,
            folder_boundaries,
        );
    }
    if !rollup.watchlist.is_empty() {
        lines.extend([String::new(), "### Watchlist".to_string(), String::new()]);
        render_candidate_tables(
            &mut lines,
            report,
            &rollup.watchlist,
            top_files,
            top_folders,
            folder_boundaries,
        );
    }

    if !rollup.findings.is_empty() {
        lines.extend([
            String::new(),
            "## Actionable Findings".to_string(),
            String::new(),
            "| Review severity | Path | Context/load band | Maintenance pressure | Why it surfaced | Next step |".to_string(),
            "| --- | --- | --- | --- | --- | --- |".to_string(),
        ]);
        for finding in rollup.findings.iter().take(top_files.max(1)) {
            lines.push(format!(
                "| {} | {} | {} | {} · score {} | {} | {} |",
                inline_code(&finding.severity),
                path_cell(report, &finding.path),
                inline_code(&finding.context_band),
                inline_code(&finding.slop_band),
                format_score(finding.slop_score),
                markdown_escape(
                    &finding
                        .reasons
                        .iter()
                        .map(|reason| format_finding_reason(reason, finding.tokens))
                        .collect::<Vec<_>>()
                        .join("; "),
                ),
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
                format_int(skipped_total),
                format_int(usize_field(stats, "skipped_ignored_count")),
                format_int(usize_field(stats, "skipped_missing_count")),
                format_int(usize_field(stats, "skipped_binary_count")),
                format_int(usize_field(stats, "skipped_undecodable_count")),
            ),
        ]);
    }
    lines.extend([
        String::new(),
        "> Git Slop reports deterministic context and maintenance-pressure evidence. Findings are not correctness proofs or automatic refactor mandates.".to_string(),
    ]);
    format!("{}\n", lines.join("\n"))
}

pub fn render_health_from_report(report: &Value) -> Result<String> {
    let rollup = health_rollup_from_report(report)?;
    Ok(render_health_value(report, &rollup))
}
