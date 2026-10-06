use anyhow::{Context, Result, bail, ensure};
use serde_json::json;
use std::{fs, path::Path};

use super::Package;
use super::common::*;
use super::git::*;
use super::proof::{read_publication, write_context};

pub(super) fn push(root: &Path, package: &Package) -> Result<()> {
    let (mut context, intent) = read_publication(root, package)?;
    ensure!(
        string_at(&context, &["publication", "stage"])? == "push-pending",
        "saved publication is not awaiting its branch push"
    );
    require_env("GH_TOKEN", "GH_TOKEN is required for the exact branch push")?;
    let repository = required_env("GITHUB_REPOSITORY")?;
    let node = value_string(&intent, "repository_node_id")?;
    let source_sha = value_string(&intent, "source_sha")?;
    let source_ref = value_string(&intent, "source_ref")?;
    let base_ref = value_string(&intent, "base_ref")?;
    let base_sha = value_string(&intent, "base_sha")?;
    let head_ref = value_string(&intent, "head_ref")?;
    let new_sha = value_string(&intent, "new_sha")?;
    let expected_old = intent["expected_old_sha"].as_str().unwrap_or("");
    let patch_sha = value_string(&intent, "patch_sha256")?;
    let files = intent["files"].clone();
    let created_at = value_string(&intent, "created_at")?;
    let title = value_string(&intent, "title")?;
    ensure!(
        remote_ref_sha(&format!("refs/heads/{base_ref}"))? == base_sha,
        "base branch changed after publication preparation"
    );
    let actual_old = remote_ref_sha(head_ref)?;
    if !expected_old.is_empty() {
        ensure!(
            actual_old == expected_old && expected_old == source_sha,
            "branch ref no longer matches the exact saved lease"
        );
    } else {
        ensure!(
            actual_old.is_empty() && source_sha == base_sha,
            "new branch is no longer absent or source differs from base"
        );
    }
    validate_git_refs(&[source_ref, &format!("refs/heads/{base_ref}"), head_ref])?;
    if intent["mode"] == "update-existing-pr" {
        let number = integer_at(&intent, &["target_pr", "number"])?;
        let current = gh_json(&["api", &format!("repos/{repository}/pulls/{number}")])?;
        let target = &intent["target_pr"];
        ensure!(
            json!({
                "number":current["number"],"url":current["html_url"],"head_repository":current["head"]["repo"]["full_name"],
                "head_ref":current["head"]["ref"],"head_sha":current["head"]["sha"],"base_repository":current["base"]["repo"]["full_name"],
                "base_ref":current["base"]["ref"],"base_sha":current["base"]["sha"],"author_login":current["user"]["login"],
                "title":current["title"],"body":current["body"].as_str().unwrap_or(""),"draft":current["draft"]
            }) == *target
                && current["state"] == "open"
                && current["head"]["repo"]["full_name"] == repository
                && current["head"]["ref"]
                    == source_ref.strip_prefix("refs/heads/").unwrap_or_default()
                && current["head"]["sha"] == source_sha
                && current["base"]["repo"]["full_name"] == repository
                && current["base"]["ref"] == base_ref
                && current["base"]["sha"] == base_sha,
            "live Dependabot PR changed since exact publication preparation"
        );
    }
    let recomputed = recompute_commit(
        &package.patch,
        source_sha,
        patch_sha,
        &files,
        created_at,
        title,
    )?;
    ensure!(
        recomputed == new_sha,
        "deterministic commit differs from the exact prepared publication plan"
    );
    let output = package.root.join("publication/push-output.txt");
    let lease = format!("--force-with-lease={head_ref}:{expected_old}");
    let mut push = git_command(&[
        "-c",
        "core.hooksPath=/dev/null",
        "-c",
        "credential.helper=",
        "-c",
        "credential.helper=!gh auth git-credential",
        "push",
        "--porcelain",
        &lease,
        "origin",
        &format!("{new_sha}:{head_ref}"),
    ])?;
    let result = push.output().context("git push could not be started")?;
    fs::write(&output, &result.stdout)?;
    if !result.status.success() {
        bail!("git push did not return a positive acknowledgement; do not replay this publication");
    }
    let ack = json!({
        "schema_version":1,"nonce":intent["nonce"],"repository":repository,
        "repository_node_id":node,"ref":head_ref,"expected_old_sha":intent["expected_old_sha"],
        "new_sha":new_sha,"positive_push_ack":true
    });
    write_json_new_or_replace(&package.root.join("publication/push-ack.json"), &ack)?;
    let (stage, steps) = if intent["mode"] == "create-pr" {
        ("pr-pending", json!(["pull-request-create"]))
    } else {
        ("pr-verify-pending", json!([]))
    };
    write_context(package, &mut context, stage, Some(ack), None, None, steps)?;
    println!("positive branch push acknowledgement recorded for {head_ref}");
    Ok(())
}
