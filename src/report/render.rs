use std::cmp::Ordering;

use serde_json::Value;

use super::support::{float_field, string_array, string_field, usize_field};
use crate::health::{github_blob_url, humanize_reason_code};
use crate::text::{inline_code, markdown_escape, visible_controls};

pub(super) const DEFAULT_SUMMARY_LIMIT: usize = 10;
const MAX_SUMMARY_LIMIT: usize = 25;

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

fn path_cell(report: &Value, path: &str) -> String {
    github_blob_url(report, path)
        .map(|url| format!("[{}]({url})", markdown_escape(path)))
        .unwrap_or_else(|| inline_code(path))
}

fn terminal_safe(value: &str) -> String {
    visible_controls(value)
}

fn summary_limit(report: &Value) -> usize {
    report
        .pointer("/config/health/summary_top_files")
        .and_then(Value::as_u64)
        .and_then(|value| usize::try_from(value).ok())
        .unwrap_or(DEFAULT_SUMMARY_LIMIT)
        .clamp(1, MAX_SUMMARY_LIMIT)
}

fn reason_text(item: &Value) -> String {
    let reasons = string_array(item.get("reason_codes"));
    if reasons.is_empty() {
        "No single dominant driver".to_string()
    } else {
        reasons
            .iter()
            .map(|reason| humanize_reason_code(reason))
            .collect::<Vec<_>>()
            .join(", ")
    }
}

fn signal_label(item: &Value) -> &'static str {
    if item
        .get("is_pure_context_hotspot")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        "context-only"
    } else {
        "mixed"
    }
}

fn array_at<'a>(value: &'a Value, pointer: &str) -> &'a [Value] {
    value
        .pointer(pointer)
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[])
}

fn render_structural_health(lines: &mut Vec<String>, report: &Value) {
    let structural = array_at(
        report,
        "/overlays/organization_health/findings/top_structural_files",
    );
    lines.extend([
        String::new(),
        "## Organization Health".to_string(),
        String::new(),
        "### Top Structural Files".to_string(),
        String::new(),
        "| Path | Duplication | Diffusion | Coupling | Boundary | Duplicate ratio | High-diffusion commits | Cross-boundary edges |".to_string(),
        "| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |".to_string(),
    ]);
    for item in structural.iter().take(5) {
        lines.push(format!(
            "| {} | {:.3} | {:.3} | {:.3} | {:.3} | {:.3} | {} | {} |",
            path_cell(report, string_field(item, "path")),
            float_field(item, "duplication_pressure"),
            float_field(item, "diffusion_pressure"),
            float_field(item, "coupling_pressure"),
            float_field(item, "boundary_pressure"),
            float_field(item, "duplicate_token_ratio"),
            usize_field(item, "high_diffusion_commit_count"),
            usize_field(item, "cross_boundary_edge_count"),
        ));
    }
    if structural.is_empty() {
        lines.push("| _none_ | 0.000 | 0.000 | 0.000 | 0.000 | 0.000 | 0 | 0 |".to_string());
    }

    let mut duplicate_pairs = array_at(
        report,
        "/overlays/organization_health/relationships/duplicate_neighborhoods",
    )
    .iter()
    .chain(
        array_at(
            report,
            "/overlays/organization_health/relationships/near_duplicate_neighborhoods",
        )
        .iter(),
    )
    .collect::<Vec<_>>();
    duplicate_pairs.sort_by(|left, right| {
        float_field(right, "evidence_score")
            .partial_cmp(&float_field(left, "evidence_score"))
            .unwrap_or(Ordering::Equal)
            .then_with(|| string_field(left, "id").cmp(string_field(right, "id")))
    });
    lines.extend([
        String::new(),
        "### Top Duplicate / Near-Duplicate Pairs".to_string(),
        String::new(),
        "| Pair | Kind | Evidence score | Boundary |".to_string(),
        "| --- | --- | ---: | --- |".to_string(),
    ]);
    for item in duplicate_pairs.into_iter().take(5) {
        let boundary = if item
            .get("crosses_top_level_boundary")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            "cross-root"
        } else {
            "local"
        };
        lines.push(format!(
            "| {} ↔ {} | {} | {:.3} | {} |",
            path_cell(report, string_field(item, "source_path")),
            path_cell(report, string_field(item, "target_path")),
            inline_code(string_field(item, "kind")),
            float_field(item, "evidence_score"),
            inline_code(boundary),
        ));
    }
    if lines.last().is_some_and(|line| line.starts_with("| ---")) {
        lines.push("| _none_ | _none_ | 0.000 | `_none_` |".to_string());
    }
}

pub fn render_compatibility_summary(report: &Value) -> String {
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
    let stats = report.get("stats").unwrap_or(&Value::Null);
    let queue = report
        .get("action_queue")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[]);
    let limit = summary_limit(report);
    let mut lines = vec![
        "# Git Slop Summary".to_string(),
        String::new(),
        format!(
            "- Repository: {}",
            inline_code(
                repo.get("repo_name")
                    .and_then(Value::as_str)
                    .unwrap_or("unknown")
            )
        ),
        format!("- Generated at: {}", inline_code(generated_at)),
        format!("- Branch: {}", inline_code(branch)),
        format!("- Head commit: {}", inline_code(head)),
    ];
    if let Some(timestamp) = analyzed_revision_at {
        lines.push(format!(
            "- Analyzed revision timestamp: {}",
            inline_code(timestamp)
        ));
    }
    lines.extend([
        format!(
            "- Analyzed files: {}",
            usize_field(stats, "analyzed_file_count")
        ),
        format!(
            "- Skipped ignored: {}",
            usize_field(stats, "skipped_ignored_count")
        ),
        format!(
            "- Skipped missing: {}",
            usize_field(stats, "skipped_missing_count")
        ),
        format!(
            "- Skipped binary: {}",
            usize_field(stats, "skipped_binary_count")
        ),
        format!(
            "- Skipped undecodable: {}",
            usize_field(stats, "skipped_undecodable_count")
        ),
        format!(
            "- Critical context files: {}",
            usize_field(stats, "critical_context_file_count")
        ),
        format!(
            "- Critical slop files: {}",
            usize_field(stats, "critical_slop_file_count")
        ),
        String::new(),
        "## Top Hotspots".to_string(),
        String::new(),
        "| Path | Slop | Context | Slop Score | Tokens | Age | Revs | Churn | Signal | Reasons |"
            .to_string(),
        "| --- | --- | --- | ---: | ---: | ---: | ---: | ---: | --- | --- |".to_string(),
    ]);
    for item in queue.iter().take(limit) {
        lines.push(format!(
            "| {} | {} | {} | {:.1} | {} | {} | {} | {:.3} | {} | {} |",
            path_cell(report, string_field(item, "path")),
            inline_code(string_field(item, "slop_band")),
            inline_code(string_field(item, "context_band")),
            float_field(item, "slop_score"),
            format_int(usize_field(item, "tokens")),
            usize_field(item, "age_days"),
            usize_field(item, "revisions_window"),
            float_field(item, "churn_pressure"),
            inline_code(signal_label(item)),
            markdown_escape(&reason_text(item)),
        ));
    }
    if queue.is_empty() {
        lines.push("| _none_ | _none_ | _none_ | 0.0 | 0 | 0 | 0 | 0.000 | `_none_` | No hotspot records |".to_string());
    }

    render_structural_health(&mut lines, report);

    let mut verification = array_at(report, "/overlays/verification/files")
        .iter()
        .collect::<Vec<_>>();
    verification.sort_by(|left, right| {
        float_field(right, "verification_gap")
            .partial_cmp(&float_field(left, "verification_gap"))
            .unwrap_or(Ordering::Equal)
            .then_with(|| string_field(left, "path").cmp(string_field(right, "path")))
    });
    lines.extend([
        String::new(),
        "## Overlay Highlights".to_string(),
        String::new(),
        "### Verification Gaps".to_string(),
        String::new(),
        "| Path | Gap | Nearby Tests | Test Cochange |".to_string(),
        "| --- | ---: | --- | ---: |".to_string(),
    ]);
    for item in verification.into_iter().take(5) {
        let nearby = string_array(item.get("nearby_test_paths"));
        let rendered_nearby = if nearby.is_empty() {
            "_none_".to_string()
        } else {
            nearby
                .iter()
                .take(3)
                .map(|path| path_cell(report, path))
                .collect::<Vec<_>>()
                .join(", ")
        };
        lines.push(format!(
            "| {} | {:.3} | {} | {:.3} |",
            path_cell(report, string_field(item, "path")),
            float_field(item, "verification_gap"),
            rendered_nearby,
            float_field(item, "test_cochange_ratio"),
        ));
    }
    if lines.last().is_some_and(|line| line.starts_with("| ---")) {
        lines.push("| _none_ | 0.000 | _none_ | 0.000 |".to_string());
    }

    lines.extend([
        String::new(),
        "## Next Action Queue".to_string(),
        String::new(),
    ]);
    if queue.is_empty() {
        lines.push("No hotspot records found.".to_string());
    } else {
        for (index, item) in queue.iter().take(limit).enumerate() {
            let path = string_field(item, "path");
            lines.push(format!(
                "{}. {} ({}, {}, slop score {:.1}, {} tokens). Next: {}",
                index + 1,
                path_cell(report, path),
                string_field(item, "slop_band"),
                string_field(item, "context_band"),
                float_field(item, "slop_score"),
                format_int(usize_field(item, "tokens")),
                inline_code(&format!("git-slop explain --path {}", shell_quote(path))),
            ));
        }
    }
    lines.extend([
        String::new(),
        "> Stable hotspot costs and additive overlay evidence remain separate. This report does not prove that a refactor is correct.".to_string(),
    ]);
    format!("{}\n", lines.join("\n"))
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

pub fn render_terminal(report: &Value) -> String {
    let health = report.get("health").unwrap_or(&Value::Null);
    let file_counts = health.get("file_band_counts").unwrap_or(&Value::Null);
    let folder_counts = health.get("folder_band_counts").unwrap_or(&Value::Null);
    let queue = report
        .get("action_queue")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[]);
    let file_total = report
        .get("files")
        .and_then(Value::as_array)
        .map(Vec::len)
        .unwrap_or_default();
    let repo = report.get("repo").unwrap_or(&Value::Null);
    let limit = summary_limit(report);
    let path_width = queue
        .iter()
        .take(limit)
        .map(|item| string_field(item, "path").chars().count())
        .max()
        .unwrap_or(4)
        .max(4);
    let mut lines = vec![
        "Repository Health".to_string(),
        format!(
            "  files: compact={} healthy={} warning={} budget_exceeded={}",
            usize_field(file_counts, "compact"),
            usize_field(file_counts, "healthy"),
            usize_field(file_counts, "warning"),
            usize_field(file_counts, "budget_exceeded"),
        ),
        format!(
            "  folders: compact={} healthy={} warning={} budget_exceeded={}",
            usize_field(folder_counts, "compact"),
            usize_field(folder_counts, "healthy"),
            usize_field(folder_counts, "warning"),
            usize_field(folder_counts, "budget_exceeded"),
        ),
        format!(
            "  git: branch={} detached={} shallow={} clean={} staged={} modified={} untracked={}",
            string_field(repo, "branch"),
            repo.get("detached_head")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            repo.get("is_shallow")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            repo.get("worktree_clean")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            usize_field(repo, "staged_change_count"),
            usize_field(repo, "modified_tracked_file_count"),
            usize_field(repo, "untracked_file_count"),
        ),
        String::new(),
    ];
    if file_total == 0 {
        lines.push("Nothing analyzed: no tracked files matched the selected scope.".to_string());
        return format!("{}\n", lines.join("\n"));
    }
    if queue.is_empty() {
        lines.push("No investigation candidates found.".to_string());
        return format!("{}\n", lines.join("\n"));
    }
    lines.push(format!(
        "{:<path_width$}  {:<8}  {:<8}  {:>9}  {:>8}  {:>5}  {:>5}  {:>6}",
        "Path", "Maint", "Context", "Score", "Tokens", "Age", "Revs", "Churn"
    ));
    lines.push(format!(
        "{:-<path_width$}  {:-<8}  {:-<8}  {:-<9}  {:-<8}  {:-<5}  {:-<5}  {:-<6}",
        "", "", "", "", "", "", "", ""
    ));
    for item in queue.iter().take(limit) {
        let path = string_field(item, "path");
        let display_path = terminal_safe(path);
        lines.push(format!(
            "{:<path_width$}  {:<8}  {:<8}  {:>9.1}  {:>8}  {:>5}  {:>5}  {:>6.3}",
            display_path,
            string_field(item, "slop_band"),
            string_field(item, "context_band"),
            float_field(item, "slop_score"),
            usize_field(item, "tokens"),
            usize_field(item, "age_days"),
            usize_field(item, "revisions_window"),
            float_field(item, "churn_pressure"),
        ));
    }
    lines.extend([
        String::new(),
        "Use `git-slop explain --path <path>` for evidence and `git-slop plan --path <path>` for a bounded maintenance proposal.".to_string(),
    ]);
    format!("{}\n", lines.join("\n"))
}
