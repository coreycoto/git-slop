use percent_encoding::{AsciiSet, CONTROLS, utf8_percent_encode};
use serde_json::Value;

use super::super::model::*;
use crate::text::{inline_code, markdown_escape};

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

pub(super) fn format_int(value: usize) -> String {
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

pub(in crate::health) fn format_number(value: f64) -> String {
    let rounded = ((value.max(0.0) * 100.0).round()) / 100.0;
    if rounded.fract().abs() < f64::EPSILON {
        return format_int(rounded as usize);
    }
    let fixed = format!("{rounded:.2}");
    let (whole, fraction) = fixed.split_once('.').unwrap_or((&fixed, "00"));
    let whole = whole.parse::<usize>().unwrap_or_default();
    format!("{}.{fraction}", format_int(whole))
}

pub(in crate::health) fn format_percent(value: f64) -> String {
    format!("{:.1}%", value * 100.0)
}

pub(in crate::health) fn format_score(value: f64) -> String {
    let fixed = format!("{:.1}", value.max(0.0));
    let (whole, fraction) = fixed.split_once('.').unwrap_or((&fixed, "0"));
    let whole = whole.parse::<usize>().unwrap_or_default();
    format!("{}.{fraction}", format_int(whole))
}

pub(in crate::health) fn format_finding_reason(reason: &str, tokens: usize) -> String {
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

pub(super) fn path_cell(report: &Value, path: &str) -> String {
    if let Some(url) = github_blob_url(report, path) {
        format!("[{}]({url})", markdown_escape(path))
    } else {
        inline_code(path)
    }
}

pub(super) fn repo_label(report: &Value) -> String {
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

pub(super) fn distribution_number(distribution: &Value, key: &str) -> f64 {
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

pub(super) fn candidate_parent_share(report: &Value, candidate: &Value) -> Option<f64> {
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
    (parent_tokens != 0).then(|| usize_field(candidate, "tokens") as f64 / parent_tokens as f64)
}
