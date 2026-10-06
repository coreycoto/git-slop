use super::*;
use tempfile::TempDir;

const SHA: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const OTHER_SHA: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

fn write_json(path: &Path, value: &Value) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, serde_json::to_vec(value).unwrap()).unwrap();
}

fn write_bytes(path: &Path, bytes: &[u8]) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, bytes).unwrap();
}

fn dispatch(operation: &str, plan_run_id: &str, plan_sha: &str) -> Value {
    json!({"inputs": {
        "operation": operation,
        "pr_number": "",
        "issue_number": "",
        "plan_run_id": plan_run_id,
        "approve_plan_sha": plan_sha,
    }})
}

fn plan(repository: &str, sha: &str) -> Value {
    let (owner, name) = repository.split_once('/').unwrap();
    json!({"schema_version":2,"command":PLAN_COMMAND,"repository":{"owner":owner,"name":name},"sha256":sha})
}

fn source_package(root: &Path, sha: &str, operation: &str) {
    let event = dispatch(operation, "", "");
    let event_bytes = serde_json::to_vec(&event).unwrap();
    let event_sha = sha256(&event_bytes);
    let preview = plan("owner/repo", sha);
    let preview_bytes = serde_json::to_vec(&preview).unwrap();
    let preview_sha = sha256(&preview_bytes);
    write_bytes(&root.join("events/trigger-event.json"), &event_bytes);
    write_bytes(&root.join("previews/execution.json"), &preview_bytes);
    write_json(
        &root.join("run-context.json"),
        &json!({
            "workflow_file":WORKFLOW_FILE,"repository":"owner/repo","recovery_key":"workflow-history-v2",
            "run_name":"Execution State Sync","workflow_run_id":12,"workflow_run_attempt":2,"phase":"completed","trusted_source_sha":OTHER_SHA,
            "dispatch_steps":[],"attempt_target":{"event_sha256":event_sha},
            "plans":[{"name":"workflow-noop","command":"workflow-noop","path":"plans/workflow-noop.json","status":"completed"}],
        }),
    );
    write_json(
        &root.join("decisions/workflow-noop.json"),
        &json!({
            "schema_version":1,"decision":"preview-only","repository":"owner/repo",
            "workflow_file":WORKFLOW_FILE,"run_id":12,"attempt":2,"recovery_key":"workflow-history-v2",
            "attempt_target":{"event_sha256":event_sha},"workflow_sha":OTHER_SHA,
            "event_name":"workflow_dispatch","event_sha256":event_sha,
            "previews":[{"path":"previews/execution.json","sha256":preview_sha}],
        }),
    );
}

fn plan_set_key(sha: &str) -> (Vec<u8>, String) {
    let bytes = canonical_target(sha).unwrap();
    let key = format!("plan-set-{}", sha256(&bytes));
    (bytes, key)
}

fn apply_args(root: &Path, submitted_run: &str) -> CheckResumedApply {
    CheckResumedApply {
        package_root: root.to_path_buf(),
        repository: "owner/repo".to_owned(),
        plan_run_id: submitted_run.to_owned(),
        approve_plan_sha: SHA.to_owned(),
        recovery_key: plan_set_key(SHA).1,
        github_output: root.parent().unwrap().join("github-output"),
    }
}

fn restored_package(
    root: &Path,
    phase: &str,
    status: &str,
    journal: bool,
) -> Vec<(PathBuf, Vec<u8>)> {
    let (target, key) = plan_set_key(SHA);
    let plan_value = plan("owner/repo", SHA);
    let plan_bytes = serde_json::to_vec(&plan_value).unwrap();
    let event_value = dispatch("apply", "12", SHA);
    let event_bytes = serde_json::to_vec(&event_value).unwrap();
    let review_value = json!({
        "schema_version":1,"workflow_file":WORKFLOW_FILE,"repository":"owner/repo",
        "plan_name":"execution","command":PLAN_COMMAND,"plan_sha256":SHA,
        "approval_input":"approve_plan_sha","workflow_event":"workflow_dispatch",
        "workflow_run_id":50,"workflow_run_attempt":1,"reviewed_plan_run_id":12,
        "event_path":"events/dispatch-event.json","event_sha256":sha256(&event_bytes),
    });
    let review_bytes = serde_json::to_vec(&review_value).unwrap();
    let journal_id = "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc";
    let entry = json!({
        "name":"execution","path":"plans/execution.json","command":PLAN_COMMAND,
        "sha256":SHA,"journal_id":journal_id,"status":status,
        "review_path":"reviews/execution.json","review_sha256":sha256(&review_bytes),
    });
    let context = json!({
        "workflow_file":WORKFLOW_FILE,"repository":"owner/repo","recovery_key":key,
        "schema_version":1,"run_name":"Execution State Sync","workflow_run_id":51,"workflow_run_attempt":1,"phase":phase,
        "recovered_from_run_id":50,"recovered_from_attempt":1,"plan_origin_run_id":50,"plan_origin_attempt":1,
        "attempt_target":serde_json::from_slice::<Value>(&target).unwrap(),"plans":[entry],
    });
    let mut files = vec![
        (
            root.join("recovery-source.json"),
            br#"{"schema_version":1}"#.to_vec(),
        ),
        (
            root.join("run-context.json"),
            serde_json::to_vec(&context).unwrap(),
        ),
        (root.join("plans/execution.json"), plan_bytes),
        (root.join("reviews/execution.json"), review_bytes),
        (root.join("events/dispatch-event.json"), event_bytes),
    ];
    if journal {
        files.push((
            root.join(format!("journal/{journal_id}.json")),
            br#"{"steps":[]}"#.to_vec(),
        ));
    }
    for (path, bytes) in &files {
        write_bytes(path, bytes);
    }
    files
}

#[test]
fn canonical_plan_set_bytes_match_the_native_sorted_target_contract() {
    let bytes = canonical_target(SHA).unwrap();
    assert_eq!(
        bytes,
        format!(
            r#"{{"plans":[{{"command":"execution-sync","name":"execution","sha256":"{SHA}"}}]}}"#
        )
        .as_bytes()
    );
}

#[test]
fn apply_input_validation_rejects_noncanonical_numbers_and_approvals() {
    let temp = TempDir::new().unwrap();
    let event = temp.path().join("event.json");
    write_json(&event, &dispatch("apply", "12", SHA));
    let valid = |run: &str, sha: &str, pr: &str| {
        validate_apply("apply".to_owned(), pr, "", run, sha, &event)
    };
    assert!(valid("01", SHA, "").is_err());
    assert!(valid("+1", SHA, "").is_err());
    assert!(valid("9223372036854775808", SHA, "").is_err());
    assert!(valid("12", &SHA.to_ascii_uppercase(), "").is_err());
    assert!(valid("12", &format!("{SHA}x"), "").is_err());
    assert!(valid("12", SHA, "17").is_err());
    assert!(validate_apply("prepare".to_owned(), "", "", "12", SHA, &event).is_err());
}

#[test]
fn apply_input_validation_binds_the_raw_dispatch_event_and_writes_exact_target() {
    let temp = TempDir::new().unwrap();
    let event = temp.path().join("event.json");
    let target = temp.path().join("target.json");
    let output = temp.path().join("github-output");
    write_json(&event, &dispatch("apply", "12", SHA));
    let recovery_key = ValidateApplyInputs {
        operation: "apply".into(),
        pr_number: String::new(),
        issue_number: String::new(),
        plan_run_id: "12".into(),
        approve_plan_sha: SHA.into(),
        event: event.clone(),
        target: target.clone(),
        github_output: output.clone(),
    }
    .run();
    assert!(recovery_key.is_ok());
    let expected = canonical_target(SHA).unwrap();
    assert_eq!(fs::read(target).unwrap(), expected);
    let written = fs::read_to_string(output).unwrap();
    assert!(written.contains(&format!("recovery_key=plan-set-{}", sha256(&expected))));
}

#[test]
fn project_snapshot_and_selector_keep_project_data_outside_native_package_logic() {
    let temp = TempDir::new().unwrap();
    let snapshot = temp.path().join("snapshot.json");
    let scope = temp.path().join("project-scope.json");
    let selector = temp.path().join("selector.json");
    write_json(
        &snapshot,
        &json!({"data":{"project":{"owner_login":"owner","owner_type":"User","title":"Backlog","number":3,"id":"PVT_123"}}}),
    );
    ProjectSnapshot {
        snapshot: snapshot.clone(),
        owner: "owner".into(),
        title: "Backlog".into(),
        number: "3".into(),
        host: "github.com".into(),
        output: scope.clone(),
    }
    .run()
    .unwrap();
    PrepareSelector {
        kind: "issue".into(),
        number: "17".into(),
        project_available: "true".into(),
        project_scope: scope.clone(),
        output: selector.clone(),
    }
    .run()
    .unwrap();
    let value: Value = read_json(&selector).unwrap();
    assert_eq!(value["issue_number"], 17);
    assert_eq!(value["pull_request_number"], 0);
    assert_eq!(value["skip_project_sync"], false);
    assert_eq!(value["project"]["id"], "PVT_123");

    let mismatch_output = temp.path().join("wrong-scope.json");
    let mut bad_snapshot = read_json(&snapshot).unwrap();
    bad_snapshot["data"]["project"]["title"] = json!("Different");
    write_json(&snapshot, &bad_snapshot);
    assert!(
        ProjectSnapshot {
            snapshot,
            owner: "owner".into(),
            title: "Backlog".into(),
            number: "3".into(),
            host: "github.com".into(),
            output: mismatch_output.clone(),
        }
        .run()
        .is_err()
    );
    assert!(!mismatch_output.exists());
}

#[test]
fn preview_noop_decision_contains_only_raw_preview_references() {
    let temp = TempDir::new().unwrap();
    let package = temp.path().join("package");
    let event_bytes = serde_json::to_vec(&dispatch("prepare", "", "")).unwrap();
    let preview_bytes = serde_json::to_vec(&plan("owner/repo", SHA)).unwrap();
    write_bytes(&package.join("events/trigger-event.json"), &event_bytes);
    write_bytes(&package.join("previews/execution.json"), &preview_bytes);
    fs::create_dir_all(package.join("decisions")).unwrap();
    let output = temp.path().join("github-output");
    fs::write(&output, "").unwrap();
    PrepareNoop {
        package_root: package.clone(),
        run_temp: temp.path().to_path_buf(),
        repository: "owner/repo".into(),
        server_url: "https://github.com".into(),
        run_name: "Execution State Sync".into(),
        workflow_sha: OTHER_SHA.into(),
        event_name: "workflow_dispatch".into(),
        run_id: "10".into(),
        attempt: "1".into(),
        github_output: output.clone(),
    }
    .run()
    .unwrap();
    let decision: Value =
        read_json(&temp.path().join("execution-state-noop-decision.json")).unwrap();
    assert!(decision.get("proposal").is_none());
    assert_eq!(
        decision["previews"],
        json!([{"path":"previews/execution.json","sha256":sha256(&preview_bytes)}])
    );
    assert_eq!(decision["event_sha256"], sha256(&event_bytes));
    let context: Value = read_json(&temp.path().join("execution-state-noop-context.json")).unwrap();
    assert_eq!(context["dispatch_steps"], json!([]));
    assert_eq!(
        context["attempt_target"],
        json!({"event_sha256":sha256(&event_bytes)})
    );
    assert!(!package.join("decisions/workflow-noop.json").exists());
    assert!(fs::read_to_string(output).unwrap().contains("ready=true"));
}

fn api_run(id: u64, conclusion: Option<&str>) -> Value {
    let mut run = json!({
        "id":id,"workflow_id":42,"repository":{"id":7,"full_name":"owner/repo"},
        "event":"workflow_dispatch","head_branch":"main","display_title":"Execution State Sync",
        "path":".github/workflows/execution_state_sync.yml@refs/heads/main",
        "run_attempt":2,"head_sha":"dddddddddddddddddddddddddddddddddddddddd",
    });
    if let Some(conclusion) = conclusion {
        run["conclusion"] = json!(conclusion);
    }
    run
}

fn handoff_artifact() -> Value {
    json!({
        "id":99,"name":"execution-state-12-run-12-attempt-2-handoff-00",
        "expired":false,"digest":format!("sha256:{}", "e".repeat(64)),
        "workflow_run":{"id":12,"repository_id":7,"head_sha":"dddddddddddddddddddddddddddddddddddddddd","head_branch":"main"},
    })
}

fn select_handoff(temp: &TempDir, selected: Value, artifacts: Vec<Value>) -> Result<()> {
    let current_path = temp.path().join("current.json");
    let selected_path = temp.path().join("selected.json");
    let artifacts_path = temp.path().join("artifacts.json");
    write_json(&current_path, &api_run(50, None));
    write_json(&selected_path, &selected);
    write_json(&artifacts_path, &json!({"artifacts":artifacts}));
    SelectHandoff {
        current_run: current_path,
        selected_run: selected_path,
        artifact_pages: artifacts_path,
        repository: "owner/repo".into(),
        current_run_id: "50".into(),
        default_branch: "main".into(),
        run_name: "Execution State Sync".into(),
        plan_run_id: "12".into(),
        output: temp.path().join("selection.json"),
    }
    .run()
}

#[test]
fn handoff_selection_requires_exact_source_identity_digest_and_uniqueness() {
    let good = api_run(12, Some("success"));
    let temp = TempDir::new().unwrap();
    select_handoff(&temp, good.clone(), vec![handoff_artifact()]).unwrap();
    let selected: Value = read_json(&temp.path().join("selection.json")).unwrap();
    assert_eq!(selected["artifact_id"], 99);
    assert_eq!(selected["source_attempt"], 2);

    let wrong_workflow = {
        let mut value = good.clone();
        value["workflow_id"] = json!(43);
        value
    };
    let wrong_repo = {
        let mut value = good.clone();
        value["repository"]["full_name"] = json!("other/repo");
        value
    };
    let wrong_branch = {
        let mut value = good.clone();
        value["head_branch"] = json!("feature");
        value
    };
    let mut bad_digest = handoff_artifact();
    bad_digest["digest"] = json!("sha256:ABC");
    for (selected, artifacts) in [
        (wrong_workflow, vec![handoff_artifact()]),
        (wrong_repo, vec![handoff_artifact()]),
        (wrong_branch, vec![handoff_artifact()]),
        (good.clone(), vec![bad_digest]),
        (good.clone(), vec![handoff_artifact(), handoff_artifact()]),
    ] {
        let temp = TempDir::new().unwrap();
        assert!(select_handoff(&temp, selected, artifacts).is_err());
        assert!(!temp.path().join("selection.json").exists());
    }
}

fn prepare_apply_fixture(temp: &TempDir, source_operation: &str) -> (PrepareApply, String) {
    let root = temp.path();
    let source = root.join("source");
    source_package(&source, SHA, source_operation);
    let current_event = root.join("current-event.json");
    let current_bytes = serde_json::to_vec(&dispatch("apply", "12", SHA)).unwrap();
    fs::write(&current_event, &current_bytes).unwrap();
    let target = root.join("target.json");
    let (target_bytes, recovery_key) = plan_set_key(SHA);
    fs::write(&target, target_bytes).unwrap();
    let package_root = root.join("apply-package");
    let run_temp = root.join("runner-temp");
    fs::create_dir_all(&run_temp).unwrap();
    let github_output = root.join("github-output");
    (
        PrepareApply {
            current_event,
            source_package: source,
            package_root,
            target,
            run_temp,
            repository: "owner/repo".into(),
            run_name: "Execution State Sync".into(),
            workflow_sha: OTHER_SHA.into(),
            current_run_id: "50".into(),
            current_attempt: "1".into(),
            plan_run_id: "12".into(),
            source_attempt: "2".into(),
            approve_plan_sha: SHA.into(),
            recovery_key,
            github_output,
        },
        root.display().to_string(),
    )
}

#[test]
fn prepare_apply_writes_the_exact_thirteen_field_review_only_after_validation() {
    let temp = TempDir::new().unwrap();
    let (args, _) = prepare_apply_fixture(&temp, "prepare");
    let package = args.package_root.clone();
    let source_run = args.source_package.clone();
    let output = args.github_output.clone();
    args.run().unwrap();
    let review: Value = read_json(&package.join("reviews/execution.json")).unwrap();
    assert_eq!(review.as_object().unwrap().len(), 13);
    assert_eq!(review["reviewed_plan_run_id"], 12);
    assert_eq!(review["event_path"], "events/dispatch-event.json");
    assert_eq!(
        review["event_sha256"],
        sha256(&fs::read(package.join("events/dispatch-event.json")).unwrap())
    );
    assert_eq!(
        read_json(&source_run.join("events/trigger-event.json")).unwrap()["inputs"]["operation"],
        "prepare"
    );
    assert!(
        fs::read_to_string(output)
            .unwrap()
            .contains("review_sha256=")
    );
}

#[test]
fn prepare_apply_rejects_a_non_prepare_transport_event_without_writing_any_apply_files() {
    let temp = TempDir::new().unwrap();
    let (args, _) = prepare_apply_fixture(&temp, "apply");
    let package = args.package_root.clone();
    let context = args
        .run_temp
        .join("execution-state-apply-context-start.json");
    let output = args.github_output.clone();
    assert!(args.run().is_err());
    assert!(!package.exists());
    assert!(!context.exists());
    assert!(!output.exists());
}

#[test]
fn resumed_apply_detects_changed_selectors_without_rewriting_retained_evidence() {
    let temp = TempDir::new().unwrap();
    let root = temp.path().join("package");
    let files = restored_package(&root, "prepared", "prepared", false);
    let before = files
        .iter()
        .map(|(path, _)| fs::read(path).unwrap())
        .collect::<Vec<_>>();
    let args = apply_args(&root, "13");
    let output_path = args.github_output.clone();
    args.run().unwrap();
    let output_text = fs::read_to_string(output_path).unwrap();
    assert!(output_text.contains("authorization_matches=false"));
    assert!(output_text.contains("journal_action=skip"));
    for ((path, _), expected) in files.iter().zip(before) {
        assert_eq!(fs::read(path).unwrap(), expected);
    }
}

#[test]
fn resumed_prepared_plan_skips_journal_but_dispatching_requires_exact_retained_journal() {
    let prepared = TempDir::new().unwrap();
    let root = prepared.path().join("package");
    restored_package(&root, "prepared", "prepared", false);
    let args = apply_args(&root, "12");
    let output_path = args.github_output.clone();
    args.run().unwrap();
    assert!(
        fs::read_to_string(output_path)
            .unwrap()
            .contains("journal_action=skip")
    );

    let dispatching = TempDir::new().unwrap();
    let root = dispatching.path().join("package");
    restored_package(&root, "dispatching", "dispatching", true);
    let args = apply_args(&root, "12");
    let output_path = args.github_output.clone();
    args.run().unwrap();
    assert!(
        fs::read_to_string(output_path)
            .unwrap()
            .contains("journal_action=install")
    );

    let missing = TempDir::new().unwrap();
    let root = missing.path().join("package");
    restored_package(&root, "dispatching", "dispatching", false);
    assert!(apply_args(&root, "12").run().is_err());
}

#[test]
fn resumed_journal_uses_native_default_but_rejects_conflicting_explicit_paths() {
    for explicit_path in [None, Some("canonical"), Some("other"), Some("null")] {
        let temp = TempDir::new().unwrap();
        let root = temp.path().join("package");
        restored_package(&root, "dispatching", "dispatching", true);
        let context_path = root.join("run-context.json");
        let mut context = read_json(&context_path).unwrap();
        let entry = &mut context["plans"][0];
        if let Some(path) = explicit_path {
            entry["journal_path"] = match path {
                "canonical" => json!(format!(
                    "journal/{}.json",
                    entry["journal_id"].as_str().unwrap()
                )),
                "other" => json!("journal/other.json"),
                _ => Value::Null,
            };
            write_json(&context_path, &context);
        }
        let before = fs::read(&context_path).unwrap();
        let args = apply_args(&root, "12");
        let output = args.github_output.clone();
        let result = args.run();
        assert_eq!(
            result.is_ok(),
            matches!(explicit_path, None | Some("canonical"))
        );
        assert_eq!(fs::read(context_path).unwrap(), before);
        if result.is_ok() {
            assert!(
                fs::read_to_string(output)
                    .unwrap()
                    .contains("journal_action=install")
            );
        } else {
            assert!(!output.exists());
        }
    }
}
