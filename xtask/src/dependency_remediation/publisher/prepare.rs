use anyhow::{Context, Result, bail, ensure};
use serde_json::{Value, json};
use std::{env, fs, path::Path};

use super::Package;
use super::common::*;
use super::git::*;
use super::proof::{validate_candidate, validate_files};

pub(super) fn prepare(root: &Path, package: &Package) -> Result<()> {
    let mut context = read_json(&package.context)?;
    ensure!(
        context.get("publication").is_none_or(Value::is_null),
        "publication intent already exists; recovery must reuse it"
    );
    validate_files(package)?;
    require_regular(
        &package.patch,
        "captured dependency patch is missing or unsafe",
    )?;
    let candidate = validate_candidate(package, true)?;
    ensure!(
        git_status()?.is_empty(),
        "trusted prepare checkout must be clean before staging the captured patch"
    );

    let event_path = required_env("GITHUB_EVENT_PATH")?;
    let live_event = fs::read(&event_path).context("the live workflow event is unavailable")?;
    ensure!(
        sha256(&live_event) == sha256(&fs::read(&package.event)?),
        "captured trigger bytes differ from the live event"
    );
    let repo = required_env("GITHUB_REPOSITORY")?;
    let event_name = required_env("GITHUB_EVENT_NAME")?;
    let event_sha = sha256(&live_event);
    let result = read_json(&package.result)?;
    let result_sha = sha256(&fs::read(&package.result)?);
    let patch_sha = sha256(&fs::read(&package.patch)?);
    let proposal_title = value_string(&result, "title")?.to_owned();
    let proposal_body = value_string(&result, "body")?.to_owned();
    let proposal_draft = result["draft"].clone();
    ensure!(
        !proposal_title.contains(['\n', '\r']),
        "pull request title contains a line break"
    );
    let publisher_login = gh_text(&["api", "user", "--jq", ".login"])?;
    let repo_node_id = gh_text(&["api", &format!("repos/{repo}"), "--jq", ".node_id"])?;
    ensure!(
        !publisher_login.is_empty() && !repo_node_id.is_empty(),
        "trusted publisher or repository identity is empty"
    );

    let (
        source_repository,
        source_ref,
        source_sha,
        base_ref,
        base_sha,
        expected_old_sha,
        head_ref,
        target_pr,
        mode,
        title,
        body,
        draft,
    ) = match event_name.as_str() {
        "pull_request_target" => {
            ensure!(
                env::var("GITHUB_ACTOR").unwrap_or_default() == "dependabot[bot]",
                "pull_request_target remediation is limited to Dependabot"
            );
            let event = read_json(&package.event)?;
            let source_repository =
                string_at(&event, &["pull_request", "head", "repo", "full_name"])?.to_owned();
            ensure!(
                source_repository == repo,
                "fork dependency branches cannot receive a trusted push"
            );
            let pr_number = integer_at(&event, &["pull_request", "number"])?;
            let source_ref = format!(
                "refs/heads/{}",
                string_at(&event, &["pull_request", "head", "ref"])?
            );
            let source_sha = string_at(&event, &["pull_request", "head", "sha"])?.to_owned();
            let base_ref = string_at(&event, &["pull_request", "base", "ref"])?.to_owned();
            let base_sha = string_at(&event, &["pull_request", "base", "sha"])?.to_owned();
            ensure!(
                string_at(&event, &["pull_request", "base", "repo", "full_name"])? == repo,
                "dependency PR base repository differs from the workflow repository"
            );
            ensure!(
                source_repository == string_at(&candidate, &["source_repository"])?
                    && source_ref.strip_prefix("refs/heads/").unwrap_or_default()
                        == string_at(&candidate, &["source_ref"])?
                    && source_sha == string_at(&candidate, &["source_sha"])?
                    && base_ref == string_at(&candidate, &["base_ref"])?
                    && base_sha == string_at(&candidate, &["base_sha"])?,
                "candidate and publisher source identities differ"
            );
            let source_branch = source_ref.strip_prefix("refs/heads/").unwrap_or_default();
            validate_sha40(&source_sha)?;
            validate_sha40(&base_sha)?;
            let current = gh_json(&["api", &format!("repos/{repo}/pulls/{pr_number}")])?;
            ensure!(
                current["state"] == "open"
                    && current["head"]["repo"]["full_name"] == repo
                    && current["head"]["ref"] == source_branch
                    && current["head"]["sha"] == source_sha
                    && current["base"]["repo"]["full_name"] == repo
                    && current["base"]["ref"] == base_ref
                    && current["base"]["sha"] == base_sha,
                "live Dependabot PR no longer matches the exact trusted event"
            );
            let expected_old_sha = json!(source_sha);
            let head_ref = source_ref.clone();
            let mode = "update-existing-pr";
            validate_git_refs(&[&source_ref, &format!("refs/heads/{base_ref}")])?;
            let target_pr = json!({
                "number": current["number"], "url": current["html_url"],
                "head_repository": current["head"]["repo"]["full_name"], "head_ref": current["head"]["ref"], "head_sha": current["head"]["sha"],
                "base_repository": current["base"]["repo"]["full_name"], "base_ref": current["base"]["ref"], "base_sha": current["base"]["sha"],
                "author_login": current["user"]["login"], "title": current["title"], "body": current["body"].as_str().unwrap_or(""), "draft": current["draft"]
            });
            let title = value_string(&target_pr, "title")?.to_owned();
            let body = value_string(&target_pr, "body")?.to_owned();
            let draft = target_pr["draft"].clone();
            (
                source_repository,
                source_ref,
                source_sha,
                base_ref,
                base_sha,
                expected_old_sha,
                head_ref,
                target_pr,
                mode,
                title,
                body,
                draft,
            )
        }
        "schedule" | "workflow_dispatch" => {
            let default_branch =
                gh_text(&["api", &format!("repos/{repo}"), "--jq", ".default_branch"])?;
            let base_ref = required_env("GITHUB_REF_NAME")?;
            ensure!(
                base_ref == default_branch,
                "scheduled and manual remediation must run from the repository default branch"
            );
            let base_sha = required_env("GITHUB_SHA")?;
            let source_sha = base_sha.clone();
            let source_ref = format!("refs/heads/{base_ref}");
            let head_ref = format!(
                "refs/heads/codex/dependency-remediation-{}",
                required_env("GITHUB_RUN_ID")?
            );
            let source_repository = repo.clone();
            ensure!(
                source_sha == candidate["source_sha"]
                    && base_ref == candidate["base_ref"]
                    && base_sha == candidate["base_sha"],
                "candidate and publisher source identities differ"
            );
            validate_git_refs(&[&source_ref, &format!("refs/heads/{base_ref}"), &head_ref])?;
            (
                source_repository,
                source_ref,
                source_sha,
                base_ref,
                base_sha,
                Value::Null,
                head_ref,
                Value::Null,
                "create-pr",
                proposal_title,
                proposal_body,
                proposal_draft,
            )
        }
        _ => bail!("unsupported dependency remediation event: {event_name}"),
    };

    let origin = git_out(Path::new("."), &["remote", "get-url", "origin"])?;
    normalize_remote(&origin)?;
    ensure!(
        remote_ref_sha(&source_ref)? == source_sha,
        "captured source branch moved before publication planning"
    );
    ensure!(
        remote_ref_sha(&format!("refs/heads/{base_ref}"))? == base_sha,
        "captured base branch moved before publication planning"
    );
    let existing_head = remote_ref_sha(&head_ref)?;
    if expected_old_sha.is_null() {
        ensure!(
            existing_head.is_empty(),
            "new dependency remediation branch already exists"
        );
    } else {
        ensure!(
            existing_head == expected_old_sha.as_str().unwrap_or_default(),
            "dependency PR branch moved before publication planning"
        );
    }

    let files = sorted_result_files(&result)?;
    let created_at = required_env("WORKFLOW_RUN_STARTED_AT")?;
    let title_for_commit = if event_name == "pull_request_target" {
        &title
    } else {
        value_string(&result, "title")?
    };
    let new_sha = recompute_commit(
        &package.patch,
        &source_sha,
        &patch_sha,
        &files,
        &created_at,
        title_for_commit,
    )?;
    let candidate_tree = git_out(
        Path::new("."),
        &["rev-parse", &format!("{new_sha}^{{tree}}")],
    )?;
    ensure!(
        candidate_tree == candidate["candidate_tree_sha"],
        "publisher candidate tree differs from the credential-free verification handoff"
    );
    let nonce = uuid_nonce()?;
    let workflow_sha = value_string(&candidate, "workflow_sha")?;
    let intent = json!({
        "schema_version":2, "repository":repo, "repository_node_id":repo_node_id,
        "workflow_file":required_env("WORKFLOW_FILE")?, "recovery_key":required_env("RECOVERY_KEY")?,
        "workflow_sha":workflow_sha, "candidate_sha256":sha256(&fs::read(&package.candidate)?),
        "run_name":required_env("WORKFLOW_RUN_NAME")?, "origin_run_id":required_env("GITHUB_RUN_ID")?.parse::<u64>()?,
        "origin_run_attempt":required_env("GITHUB_RUN_ATTEMPT")?.parse::<u64>()?, "event_name":event_name,
        "trigger_event_sha256":event_sha, "source_repository":source_repository, "source_ref":source_ref,
        "source_sha":source_sha, "target_pr":target_pr, "mode":mode, "base_ref":base_ref,
        "base_sha":base_sha, "head_ref":head_ref, "expected_old_sha":expected_old_sha,
        "new_sha":new_sha, "publisher_login":publisher_login, "title":title, "body":body,
        "draft":draft, "nonce":nonce, "patch_sha256":patch_sha, "result_sha256":result_sha,
        "files":files, "created_at":created_at
    });
    write_json_new_or_replace(&package.intent, &intent)?;
    let intent_sha = canonical_sha(root, &package.intent)?;
    let publication = json!({
        "intent_sha256":intent_sha,"origin_run_id":intent["origin_run_id"],
        "origin_run_attempt":intent["origin_run_attempt"],"stage":"push-pending",
        "push_ack":null,"pr_ack":null,"verify_ack":null
    });
    write_json_new_or_replace(&package.root.join("publication/context.json"), &publication)?;
    let steps = json!(["branch-push"]);
    context["phase"] = json!("prepared");
    context["dispatch_steps"] = steps;
    context["publication"] = publication;
    write_json_new_or_replace(&package.context, &context)?;
    println!("prepared dependency publication {intent_sha}");
    Ok(())
}
