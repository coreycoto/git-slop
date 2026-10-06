use anyhow::{Result, bail, ensure};
use serde_json::json;
use std::{fs, path::Path};

use super::Package;
use super::common::*;
use super::git::*;
use super::proof::{read_publication, write_context};

pub(super) fn create_pr(root: &Path, package: &Package) -> Result<()> {
    let (mut context, intent) = read_publication(root, package)?;
    ensure!(
        string_at(&context, &["publication", "stage"])? == "pr-pending",
        "saved publication is not awaiting PR creation"
    );
    require_env("GH_TOKEN", "GH_TOKEN is required for exact PR creation")?;
    let repo = required_env("GITHUB_REPOSITORY")?;
    let node = value_string(&intent, "repository_node_id")?;
    let head_ref = value_string(&intent, "head_ref")?;
    let head = head_ref.strip_prefix("refs/heads/").unwrap_or_default();
    let base = value_string(&intent, "base_ref")?;
    let new_sha = value_string(&intent, "new_sha")?;
    let title = value_string(&intent, "title")?;
    let body = value_string(&intent, "body")?;
    let live_node = gh_text(&["api", &format!("repos/{repo}"), "--jq", ".node_id"])?;
    ensure!(
        live_node == node,
        "target repository identity changed before PR publication"
    );
    validate_git_refs(&[&format!("refs/heads/{base}"), &format!("refs/heads/{head}")])?;
    ensure!(
        remote_ref_sha(&format!("refs/heads/{base}"))? == intent["base_sha"],
        "base branch changed after branch publication"
    );
    ensure!(
        remote_ref_sha(head_ref)? == new_sha,
        "branch acknowledgement cannot be matched to the exact remote commit"
    );
    let prs = gh_json(&[
        "pr",
        "list",
        "--repo",
        &repo,
        "--state",
        "open",
        "--head",
        head,
        "--base",
        base,
        "--json",
        "number,url",
    ])?;
    ensure!(
        prs.as_array().is_some_and(Vec::is_empty),
        "an open PR appeared after exact create intent; do not create or adopt it"
    );
    let mut args = vec![
        "pr",
        "create",
        "--repo",
        repo.as_str(),
        "--head",
        head,
        "--base",
        base,
        "--title",
        title,
        "--body",
        body,
    ];
    if intent["draft"] == true {
        args.push("--draft");
    }
    let output = package.root.join("publication/create-pr-output.txt");
    let result = gh_output(&args)?;
    fs::write(&output, &result.stdout)?;
    if !result.status.success() {
        bail!("gh pr create did not return a positive acknowledgement; do not replay PR creation");
    }
    let url = String::from_utf8(result.stdout)?.trim().to_owned();
    let server = required_env("GITHUB_SERVER_URL")?
        .trim_end_matches('/')
        .to_owned();
    let prefix = format!("{server}/{repo}/pull/");
    ensure!(
        url.starts_with(&prefix),
        "gh pr create stdout did not contain one exact same-repository PR URL"
    );
    let number = url.strip_prefix(&prefix).unwrap_or_default();
    ensure!(
        positive_integer(number).is_ok() && url == format!("{prefix}{number}"),
        "gh pr create stdout did not contain one exact same-repository PR URL"
    );
    let ack = json!({
        "schema_version":1,"nonce":intent["nonce"],"repository":repo,"repository_node_id":node,
        "url":url,"number":number.parse::<u64>()?,"head_ref":head,"head_sha":new_sha,"base_ref":base,
        "base_sha":intent["base_sha"],"publisher_login":intent["publisher_login"],
        "title":title,"body":body,"draft":intent["draft"]
    });
    write_json_new_or_replace(&package.root.join("publication/pr-ack.json"), &ack)?;
    let push_ack = context["publication"]["push_ack"].clone();
    write_context(
        package,
        &mut context,
        "pr-verify-pending",
        Some(push_ack),
        Some(ack),
        None,
        json!([]),
    )?;
    println!("positive PR creation acknowledgement recorded for {url}");
    Ok(())
}

pub(super) fn verify_pr(root: &Path, package: &Package) -> Result<()> {
    let (mut context, intent) = read_publication(root, package)?;
    ensure!(
        string_at(&context, &["publication", "stage"])? == "pr-verify-pending",
        "saved publication is not awaiting read-only PR verification"
    );
    require_env("GH_TOKEN", "GH_TOKEN is required for exact PR verification")?;
    let repo = required_env("GITHUB_REPOSITORY")?;
    let node = gh_text(&["api", &format!("repos/{repo}"), "--jq", ".node_id"])?;
    ensure!(
        node == intent["repository_node_id"],
        "target repository immutable identity changed"
    );
    let (number, url, author, title, body, draft) = if !context["publication"]["pr_ack"].is_null() {
        let ack = &context["publication"]["pr_ack"];
        (
            integer_at(ack, &["number"])?,
            value_string(ack, "url")?.to_owned(),
            value_string(ack, "publisher_login")?.to_owned(),
            value_string(ack, "title")?.to_owned(),
            value_string(ack, "body")?.to_owned(),
            ack["draft"].clone(),
        )
    } else {
        let target = &intent["target_pr"];
        (
            integer_at(target, &["number"])?,
            value_string(target, "url")?.to_owned(),
            value_string(target, "author_login")?.to_owned(),
            value_string(target, "title")?.to_owned(),
            value_string(target, "body")?.to_owned(),
            target["draft"].clone(),
        )
    };
    let current = gh_json(&[
        "pr",
        "view",
        &number.to_string(),
        "--repo",
        &repo,
        "--json",
        "number,url,headRefName,headRefOid,baseRefName,author,title,body,isDraft,state",
    ])?;
    let head = value_string(&intent, "head_ref")?
        .strip_prefix("refs/heads/")
        .unwrap_or_default()
        .to_owned();
    ensure!(
        current["state"] == "OPEN"
            && current["url"] == url
            && current["headRefName"] == head
            && current["headRefOid"] == intent["new_sha"]
            && current["baseRefName"] == intent["base_ref"]
            && current["author"]["login"] == author
            && current["title"] == title
            && current["body"].as_str().unwrap_or("") == body
            && current["isDraft"] == draft,
        "live PR differs from the exact saved publication intent"
    );
    let ack = json!({
        "schema_version":1,"nonce":intent["nonce"],"repository":repo,"repository_node_id":node,
        "url":url,"number":number,"head_ref":head,"head_sha":intent["new_sha"],"base_ref":intent["base_ref"],
        "author_login":author,"title":title,"body":body,"draft":draft,"state":"OPEN","verified":true
    });
    write_json_new_or_replace(&package.root.join("publication/verify-ack.json"), &ack)?;
    let push_ack = context["publication"]["push_ack"].clone();
    let pr_ack = context["publication"]["pr_ack"].clone();
    write_context(
        package,
        &mut context,
        "completed",
        Some(push_ack),
        Some(pr_ack),
        Some(ack),
        json!([]),
    )?;
    write_json_new_or_replace(
        &package.root.join("apply-result.json"),
        &json!({
            "schema_version":1,"status":"completed","intent_sha256":context["publication"]["intent_sha256"],
            "url":url,"number":number,"mutations_replayed":false
        }),
    )?;
    println!("verified exact dependency PR {url}");
    Ok(())
}
