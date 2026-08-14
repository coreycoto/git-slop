use std::path::Path;

use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::{VERSION, analyze, config, git};

pub(crate) const DEFAULT_MAX_REPORT_AGE_SECONDS: i64 = 24 * 60 * 60;

#[derive(Debug, Clone, Serialize)]
pub(crate) struct FreshnessReason {
    pub code: &'static str,
    pub pointer: &'static str,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct ReportFreshness {
    pub status: &'static str,
    pub current: bool,
    pub age_seconds: Option<i64>,
    pub max_age_seconds: i64,
    pub reasons: Vec<FreshnessReason>,
}

impl ReportFreshness {
    pub(crate) fn reason_codes(&self) -> String {
        self.reasons
            .iter()
            .map(|reason| reason.code)
            .collect::<Vec<_>>()
            .join(", ")
    }
}

fn digest_value(value: &Value) -> String {
    hex::encode(Sha256::digest(
        serde_json::to_vec(value).unwrap_or_default(),
    ))
}

fn runtime_exclusions(repo_root: &Path) -> Vec<String> {
    [
        config::cache_dir(repo_root),
        config::latest_dir(repo_root),
        config::runs_dir(repo_root),
        config::slop_dir(repo_root).join("scan.lock"),
        config::slop_dir(repo_root).join("scan.lock.owner"),
    ]
    .into_iter()
    .filter_map(|path| {
        path.strip_prefix(repo_root)
            .ok()
            .map(|value| value.to_string_lossy().replace('\\', "/"))
    })
    .collect()
}

fn reason(
    code: &'static str,
    pointer: &'static str,
    message: impl Into<String>,
) -> FreshnessReason {
    FreshnessReason {
        code,
        pointer,
        message: message.into(),
    }
}

pub(crate) fn evaluate(repo_root: &Path, report: &Value) -> Result<ReportFreshness> {
    let mut reasons = Vec::new();
    let repo = git::repo_metadata(repo_root)?;
    let report_head = report.pointer("/repo/head_sha").and_then(Value::as_str);
    if report_head != repo.head_commit.as_deref() {
        reasons.push(reason(
            "head_changed",
            "/repo/head_sha",
            format!(
                "report revision {} does not match current revision {}",
                report_head.unwrap_or("unborn"),
                repo.head_commit.as_deref().unwrap_or("unborn")
            ),
        ));
    }

    let worktree = git::worktree_state_excluding(repo_root, &runtime_exclusions(repo_root))?;
    let report_worktree = report
        .pointer("/repo/worktree_state_digest")
        .and_then(Value::as_str);
    if report_worktree != Some(worktree.digest.as_str()) {
        reasons.push(reason(
            "worktree_changed",
            "/repo/worktree_state_digest",
            "tracked, staged, or untracked repository state changed since analysis",
        ));
    }

    let report_version = report.pointer("/analyzer/version").and_then(Value::as_str);
    if report_version != Some(VERSION) {
        reasons.push(reason(
            "analyzer_changed",
            "/analyzer/version",
            format!(
                "report analyzer {} does not match installed analyzer {VERSION}",
                report_version.unwrap_or("unknown")
            ),
        ));
    }

    match config::load(repo_root) {
        Ok(current_config) => {
            let current_digest = digest_value(&current_config);
            let report_digest = report
                .pointer("/analyzer/config_digest")
                .and_then(Value::as_str);
            if report_digest != Some(current_digest.as_str()) {
                reasons.push(reason(
                    "config_changed",
                    "/analyzer/config_digest",
                    "effective configuration changed since analysis",
                ));
            }
        }
        Err(error) => reasons.push(reason(
            "config_unavailable",
            "/analyzer/config_digest",
            format!("current configuration is invalid: {error:#}"),
        )),
    }

    let scope = report.pointer("/scope/path").and_then(Value::as_str);
    let tracked_paths = git::list_tracked_files(repo_root)?
        .into_iter()
        .filter(|path| {
            scope.is_none_or(|scope| path == scope || path.starts_with(&format!("{scope}/")))
        })
        .collect::<Vec<_>>();
    let report_path_digest = report
        .pointer("/scope/selected_path_digest")
        .and_then(Value::as_str);
    let degraded = report
        .pointer("/diagnostics/analysis/degraded_omitted_path_count")
        .and_then(Value::as_u64)
        .unwrap_or_default()
        > 0;
    if !degraded {
        let current_path_digest = analyze::selected_path_digest(&tracked_paths);
        if report_path_digest != Some(current_path_digest.as_str()) {
            reasons.push(reason(
                "scope_changed",
                "/scope/selected_path_digest",
                "the tracked paths selected by the report scope changed since analysis",
            ));
        }
    }

    let generated_at = report
        .get("generated_at")
        .and_then(Value::as_str)
        .and_then(|value| DateTime::parse_from_rfc3339(value).ok())
        .map(|value| value.with_timezone(&Utc));
    let age_seconds =
        generated_at.map(|value| Utc::now().signed_duration_since(value).num_seconds());
    match age_seconds {
        Some(age) if age > DEFAULT_MAX_REPORT_AGE_SECONDS => reasons.push(reason(
            "report_too_old",
            "/generated_at",
            format!(
                "report age is {age} seconds; the currentness window is {} seconds",
                DEFAULT_MAX_REPORT_AGE_SECONDS
            ),
        )),
        Some(age) if age < -300 => reasons.push(reason(
            "report_from_future",
            "/generated_at",
            "report timestamp is more than five minutes in the future",
        )),
        None => reasons.push(reason(
            "report_age_unknown",
            "/generated_at",
            "report timestamp could not be evaluated",
        )),
        Some(_) => {}
    }

    Ok(ReportFreshness {
        status: if reasons.is_empty() {
            "current"
        } else {
            "stale"
        },
        current: reasons.is_empty(),
        age_seconds,
        max_age_seconds: DEFAULT_MAX_REPORT_AGE_SECONDS,
        reasons,
    })
}
