use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result, bail};
use serde_json::{Value, json};

const REPOSITORY: &str = "coreycoto/git-slop";

fn gh_json(repo_root: &Path, endpoint: &str) -> Result<Value> {
    let output = Command::new("gh")
        .current_dir(repo_root)
        .args(["api", endpoint])
        .output()
        .with_context(|| format!("failed to query GitHub endpoint {endpoint}"))?;
    if !output.status.success() {
        bail!(
            "gh api {endpoint} exited with {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    serde_json::from_slice(&output.stdout)
        .with_context(|| format!("GitHub endpoint {endpoint} returned invalid JSON"))
}

fn run_summary(value: Option<&Value>) -> Value {
    let Some(value) = value else {
        return Value::Null;
    };
    json!({
        "id": value.get("id").cloned().unwrap_or(Value::Null),
        "status": value.get("status").cloned().unwrap_or(Value::Null),
        "conclusion": value.get("conclusion").cloned().unwrap_or(Value::Null),
        "url": value.get("html_url").cloned().unwrap_or(Value::Null),
        "created_at": value.get("created_at").cloned().unwrap_or(Value::Null),
        "head_sha": value.get("head_sha").cloned().unwrap_or(Value::Null),
    })
}

fn workflow_run_by_title(
    repo_root: &Path,
    repository: &str,
    workflow: &str,
    title: &str,
) -> Result<Value> {
    let endpoint = format!("repos/{repository}/actions/workflows/{workflow}/runs?per_page=100");
    let payload = gh_json(repo_root, &endpoint)?;
    Ok(run_summary(
        payload
            .get("workflow_runs")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .find(|run| run.get("display_title").and_then(Value::as_str) == Some(title)),
    ))
}

fn workflow_run_by_revision(
    repo_root: &Path,
    workflow: &str,
    revision: Option<&str>,
) -> Result<Value> {
    let endpoint = format!("repos/{REPOSITORY}/actions/workflows/{workflow}/runs?per_page=100");
    let payload = gh_json(repo_root, &endpoint)?;
    Ok(run_summary(
        payload
            .get("workflow_runs")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .find(|run| {
                revision.is_some_and(|revision| {
                    run.get("head_sha").and_then(Value::as_str) == Some(revision)
                })
            }),
    ))
}

fn successful(run: &Value) -> bool {
    run.get("status").and_then(Value::as_str) == Some("completed")
        && run.get("conclusion").and_then(Value::as_str) == Some("success")
}

pub fn inspect(repo_root: &Path, version: &str, json_output: bool) -> Result<()> {
    if !crate::manifest::is_strict_semver(version) {
        bail!("release status requires a strict semver version, received {version}");
    }
    let tag = format!("v{version}");
    let releases = gh_json(
        repo_root,
        &format!("repos/{REPOSITORY}/releases?per_page=100"),
    )?;
    let release = releases
        .as_array()
        .into_iter()
        .flatten()
        .find(|release| release.get("tag_name").and_then(Value::as_str) == Some(tag.as_str()));
    let revision = gh_json(repo_root, &format!("repos/{REPOSITORY}/commits/{tag}"))
        .ok()
        .and_then(|commit| {
            commit
                .get("sha")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned)
        });
    let publish = workflow_run_by_revision(repo_root, "release-publish.yml", revision.as_deref())?;
    let publication = workflow_run_by_title(repo_root, REPOSITORY, "release-published.yml", &tag)?;
    let receiver_title = format!("Update git-slop to {version}");
    let homebrew = workflow_run_by_title(
        repo_root,
        "coreycoto/homebrew-tap",
        "update-git-slop.yml",
        &receiver_title,
    )?;
    let scoop = workflow_run_by_title(
        repo_root,
        "coreycoto/scoop-bucket",
        "update-git-slop.yml",
        &receiver_title,
    )?;

    let release_summary = release.map_or(Value::Null, |release| {
        json!({
            "id": release.get("id").cloned().unwrap_or(Value::Null),
            "url": release.get("html_url").cloned().unwrap_or(Value::Null),
            "draft": release.get("draft").cloned().unwrap_or(Value::Null),
            "prerelease": release.get("prerelease").cloned().unwrap_or(Value::Null),
            "immutable": release.get("immutable").cloned().unwrap_or(Value::Null),
            "created_at": release.get("created_at").cloned().unwrap_or(Value::Null),
            "published_at": release.get("published_at").cloned().unwrap_or(Value::Null),
        })
    });
    let is_draft = release_summary.get("draft").and_then(Value::as_bool) == Some(true);
    let is_public_immutable = release_summary.get("draft").and_then(Value::as_bool) == Some(false)
        && release_summary.get("prerelease").and_then(Value::as_bool) == Some(false)
        && release_summary.get("immutable").and_then(Value::as_bool) == Some(true);
    let marketplace_ready = is_draft && successful(&publish);
    let status = if is_public_immutable
        && successful(&publication)
        && successful(&homebrew)
        && successful(&scoop)
    {
        "complete"
    } else if marketplace_ready {
        "awaiting-marketplace-publication"
    } else if release.is_none() && publish.is_null() {
        "not-started"
    } else {
        "in-progress-or-blocked"
    };
    let receipt = json!({
        "schema_version": 1,
        "command": "release-status",
        "status": status,
        "repository": REPOSITORY,
        "version": version,
        "tag": tag,
        "revision": revision,
        "release": release_summary,
        "marketplace_ready": marketplace_ready,
        "public_immutable": is_public_immutable,
        "workflows": {
            "release_publish": publish,
            "publication_verification": publication,
            "homebrew_receiver": homebrew,
            "scoop_receiver": scoop,
        }
    });
    if json_output {
        println!("{}", serde_json::to_string_pretty(&receipt)?);
        return Ok(());
    }

    println!("Release status: {tag}");
    println!("- overall: {status}");
    println!(
        "- revision: {}",
        receipt
            .get("revision")
            .and_then(Value::as_str)
            .unwrap_or("not available")
    );
    if release_summary.is_null() {
        println!("- GitHub release: not found");
    } else {
        println!(
            "- GitHub release: id={} draft={} immutable={} {}",
            release_summary
                .get("id")
                .map(Value::to_string)
                .unwrap_or_else(|| "unknown".into()),
            release_summary
                .get("draft")
                .map(Value::to_string)
                .unwrap_or_else(|| "unknown".into()),
            release_summary
                .get("immutable")
                .map(Value::to_string)
                .unwrap_or_else(|| "unknown".into()),
            release_summary
                .get("url")
                .and_then(Value::as_str)
                .unwrap_or("")
        );
    }
    println!("- Marketplace handoff ready: {marketplace_ready}");
    println!("- public immutable release: {is_public_immutable}");
    for (label, run) in [
        ("release workflow", &publish),
        ("publication verification", &publication),
        ("Homebrew receiver", &homebrew),
        ("Scoop receiver", &scoop),
    ] {
        println!(
            "- {label}: {} {}",
            run.get("conclusion")
                .or_else(|| run.get("status"))
                .and_then(Value::as_str)
                .unwrap_or("not found"),
            run.get("url").and_then(Value::as_str).unwrap_or("")
        );
    }
    Ok(())
}
