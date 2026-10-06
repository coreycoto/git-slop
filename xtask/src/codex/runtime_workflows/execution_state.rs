use serde_yaml::Value as YamlValue;

use super::{
    EXECUTION_APPLY, EXECUTION_PREPARE, PROJECT_SNAPSHOT, PROJECT_TOKEN, WorkflowStepView,
    validate_step_token, yaml_contains, yaml_mapping_has_key,
};

pub(crate) fn validate_policy_text(source: &str, workflow_source: &str, errors: &mut Vec<String>) {
    let policy: serde_json::Value = match serde_json::from_str(source) {
        Ok(policy) => policy,
        Err(error) => {
            errors.push(format!(
                "execution-state native recovery policy is invalid JSON: {error}"
            ));
            return;
        }
    };
    let workflow = &policy["workflows"]["execution_state_sync.yml"];
    let plan = &workflow["plans"]["execution"];
    if workflow["allow_publication"] != serde_json::json!(false)
        || workflow["reviewed_source_shas"] != serde_json::json!([])
        || plan["command"] != "execution-sync"
        || plan["domain_profile"] != "execution"
        || plan["attempt_target"] != "plan_set"
        || plan["approval"]
            != serde_json::json!({
                "kind": "reviewed-dispatch", "approval_input": "approve_plan_sha",
                "reviewed_run_input": "plan_run_id", "required_inputs": {"operation": "apply"}
            })
        || plan["event"]
            != serde_json::json!({"kind": "workflow_dispatch", "path": "events/dispatch-event.json"})
    {
        errors.push("execution-state must bind the exact reviewed manual dispatch without granting legacy source or publication authority.".into());
    }
    let mutators = serde_json::json!([{
        "job": "Apply separately approved execution-state plan",
        "steps": ["Apply exact reviewed execution plan"]
    }]);
    use sha2::{Digest, Sha256};
    let digest = hex::encode(Sha256::digest(workflow_source.as_bytes()));
    if plan["prepared_recovery"]["mutators"] != mutators
        || workflow["plans"]["workflow-noop"]["approval"]["mutators"] != mutators
        || plan["prepared_recovery"]["workflow_source_sha256"] != digest
        || workflow["plans"]["workflow-noop"]["approval"]["workflow_source_sha256"] != digest
    {
        errors.push("execution-state native preparation and no-op proof must bind the exact workflow bytes and independent apply step.".into());
    }
}

pub(super) fn validate_trust(
    text: &str,
    payload: &YamlValue,
    steps: &[WorkflowStepView],
    errors: &mut Vec<String>,
) {
    let name = "execution_state_sync.yml";
    let manual = payload.get("on").and_then(YamlValue::as_mapping);
    if manual.is_none_or(|on| {
        on.len() != 1 || !on.contains_key(YamlValue::String("workflow_dispatch".into()))
    }) {
        errors.push(format!(
            "{name} must support only manual workflow_dispatch."
        ));
    }
    let inputs = payload
        .get("on")
        .and_then(|on| on.get("workflow_dispatch"))
        .and_then(|dispatch| dispatch.get("inputs"))
        .and_then(YamlValue::as_mapping);
    let expected_inputs = [
        "operation",
        "pr_number",
        "issue_number",
        "plan_run_id",
        "approve_plan_sha",
    ];
    if inputs.is_none_or(|inputs| {
        inputs.len() != expected_inputs.len()
            || expected_inputs
                .iter()
                .any(|key| !inputs.contains_key(YamlValue::String((*key).into())))
    }) {
        errors.push(format!(
            "{name} must expose the five reviewed manual operation inputs."
        ));
    } else if let Some(inputs) = inputs {
        let operation = &inputs[YamlValue::String("operation".into())];
        if operation.get("type").and_then(YamlValue::as_str) != Some("choice")
            || operation.get("default").and_then(YamlValue::as_str) != Some("prepare")
            || operation.get("required").and_then(YamlValue::as_bool) != Some(true)
            || operation.get("options") != Some(&serde_yaml::from_str("[prepare, apply]").unwrap())
            || expected_inputs[1..].iter().any(|key| {
                let input = &inputs[YamlValue::String((*key).into())];
                input.get("type").and_then(YamlValue::as_str) != Some("string")
                    || input.get("required").and_then(YamlValue::as_bool) != Some(false)
            })
        {
            errors.push(format!(
                "{name} must default to prepare and accept optional exact approval inputs."
            ));
        }
    }
    if payload
        .get("concurrency")
        .and_then(|v| v.get("cancel-in-progress"))
        .and_then(YamlValue::as_bool)
        != Some(false)
        || payload
            .get("concurrency")
            .and_then(|v| v.get("queue"))
            .and_then(YamlValue::as_str)
            != Some("max")
    {
        errors.push(format!(
            "{name} must queue and preserve in-flight mutation runs."
        ));
    }
    if payload.get("run-name").and_then(YamlValue::as_str) != Some("Execution State Sync") {
        errors.push(format!(
            "{name} must keep a stable run name for exact recovery identity."
        ));
    }
    let trusted_ref = concat!("$", "{{ github.workflow_sha }}");
    let checkouts = steps
        .iter()
        .filter(|step| step.uses.starts_with("actions/checkout@"))
        .collect::<Vec<_>>();
    if checkouts.len() != 2
        || checkouts
            .iter()
            .any(|step| step.checkout_ref != trusted_ref || step.persist_credentials != Some(false))
    {
        errors.push(format!("{name} must check out only the exact trusted workflow source with credentials disabled."));
    }
    if payload.get("env").is_some_and(has_credentials) {
        errors.push(format!(
            "{name} must not expose credentials at workflow scope."
        ));
    }
    let Some(jobs) = payload.get("jobs").and_then(YamlValue::as_mapping) else {
        return;
    };
    if jobs.len() != 2 {
        errors.push(format!(
            "{name} must isolate two independent manual preparation and application jobs."
        ));
    }
    for operation in ["prepare", "apply"] {
        let Some(job) = jobs.get(YamlValue::String(operation.into())) else {
            errors.push(format!(
                "{name} must isolate preparation from native application."
            ));
            continue;
        };
        let expected = format!(
            "github.event_name == 'workflow_dispatch' && inputs.operation == '{operation}' && github.ref == format('refs/heads/{{0}}', github.event.repository.default_branch)"
        );
        let condition = job.get("if").and_then(YamlValue::as_str).unwrap_or("");
        let normalized = |value: &str| {
            value
                .chars()
                .filter(|c| !c.is_whitespace())
                .collect::<String>()
        };
        let condition = condition
            .trim()
            .strip_prefix("${{")
            .and_then(|s| s.strip_suffix("}}"))
            .unwrap_or(condition);
        if normalized(condition) != normalized(&expected) || job.get("needs").is_some() {
            errors.push(format!("{name} {operation} job must independently require its matching manual operation on the default branch."));
        }
        if job.get("env").is_some_and(has_credentials) {
            errors.push(format!(
                "{name} {operation} job must not expose credentials through job environment."
            ));
        }
    }
    let prepare_permissions = jobs
        .get(YamlValue::String("prepare".into()))
        .and_then(|job| job.get("permissions"))
        .and_then(YamlValue::as_mapping);
    if prepare_permissions.is_none_or(|permissions| {
        permissions
            .values()
            .any(|value| matches!(value.as_str(), Some("write" | "admin")))
    }) {
        errors.push(format!("{name} prepare job must remain read-only."));
    }
    for scope in ["contents", "actions"] {
        if prepare_permissions
            .and_then(|p| p.get(YamlValue::String(scope.into())))
            .and_then(YamlValue::as_str)
            != Some("read")
        {
            errors.push(format!("{name} prepare job must have {scope}: read."));
        }
    }
    for command in [PROJECT_SNAPSHOT, EXECUTION_PREPARE, EXECUTION_APPLY] {
        let matching = steps
            .iter()
            .filter(|step| step.run.contains(command))
            .collect::<Vec<_>>();
        let expected_job = if command == EXECUTION_APPLY {
            "apply"
        } else {
            "prepare"
        };
        if matching.len() != 1 || matching[0].job != expected_job {
            errors.push(format!(
                "{name} must define exactly one {command} operation in the {expected_job} job."
            ));
        } else {
            validate_step_token(matching[0], PROJECT_TOKEN, name, errors);
        }
    }
    for step in steps {
        if step.raw.get("env").is_some_and(has_credentials) {
            let project = [PROJECT_SNAPSHOT, EXECUTION_PREPARE, EXECUTION_APPLY]
                .iter()
                .any(|command| step.run.contains(command));
            let recovery = [
                "runs recover",
                "runs acquire-handoff",
                "runs finish-noop",
                "runs qualify-prepared",
                "finalize-gh-steward-run",
                "gh api",
            ]
            .iter()
            .any(|command| step.run.contains(command));
            if !project && !recovery {
                errors.push(format!("{name} must expose GitHub tokens only to scoped recovery, handoff, Project, or native execution steps."));
            } else if !project {
                validate_step_token(step, concat!("$", "{{ github.token }}"), name, errors);
            }
        }
    }
    if steps
        .iter()
        .any(|step| step.job == "prepare" && step.run.contains(" execution apply"))
        || !text.contains("RUN_NAME: Execution State Sync")
    {
        errors.push(format!(
            "{name} must keep preparation read-only with explicit stable recovery identity."
        ));
    }
}

fn has_credentials(value: &YamlValue) -> bool {
    yaml_mapping_has_key(value, "GH_TOKEN")
        || yaml_mapping_has_key(value, "GITHUB_TOKEN")
        || yaml_contains(value, "secrets.")
}

pub(super) fn validate_artifacts(text: &str, steps: &[WorkflowStepView], errors: &mut Vec<String>) {
    let name = "execution_state_sync.yml";
    let by_id = |job: &str, id: &str| {
        steps.iter().find(|step| {
            step.job == job && step.raw.get("id").and_then(YamlValue::as_str) == Some(id)
        })
    };
    let native = |job: &str, command: &str| {
        steps
            .iter()
            .find(|step| step.job == job && step.run.contains(command))
    };
    let (
        Some(prepare_recovery),
        Some(target),
        Some(preview),
        Some(noop_context),
        Some(finish_noop),
        Some(prepare_upload),
        Some(handoff_upload),
        Some(prepare_finalizer),
        Some(apply_inputs),
        Some(apply_recovery),
        Some(source),
        Some(review),
        Some(register),
        Some(restored),
        Some(install_journal),
        Some(dispatch),
        Some(apply),
        Some(capture),
        Some(qualify),
        Some(apply_upload),
        Some(apply_finalizer),
    ) = (
        by_id("prepare", "recovery"),
        by_id("prepare", "target"),
        by_id("prepare", "execution_plan"),
        by_id("prepare", "noop_context"),
        by_id("prepare", "finish_noop"),
        by_id("prepare", "terminal_artifact"),
        by_id("prepare", "handoff_artifact"),
        by_id("prepare", "finalize_checkpoint"),
        by_id("apply", "apply_inputs"),
        by_id("apply", "recovery"),
        by_id("apply", "source_plan"),
        by_id("apply", "review_context"),
        by_id("apply", "fresh_context"),
        by_id("apply", "resumed_plan"),
        native("apply", "runs context-install-journal"),
        by_id("apply", "mark_dispatching"),
        by_id("apply", "execution_apply"),
        native("apply", "runs context-capture-journal"),
        by_id("apply", "qualify_prepared"),
        by_id("apply", "terminal_artifact"),
        by_id("apply", "finalize_checkpoint"),
    )
    else {
        errors.push(format!("{name} must retain separate native preview, recovery, reviewed handoff, approval, journal, apply and checkpoint steps."));
        return;
    };
    if !(prepare_recovery.ordinal < target.ordinal
        && target.ordinal < preview.ordinal
        && preview.ordinal < noop_context.ordinal
        && noop_context.ordinal < finish_noop.ordinal
        && finish_noop.ordinal < prepare_upload.ordinal
        && prepare_upload.ordinal < handoff_upload.ordinal
        && handoff_upload.ordinal < prepare_finalizer.ordinal
        && apply_inputs.ordinal < apply_recovery.ordinal
        && apply_recovery.ordinal < source.ordinal
        && source.ordinal < review.ordinal
        && review.ordinal < register.ordinal
        && restored.ordinal < install_journal.ordinal
        && install_journal.ordinal < dispatch.ordinal
        && register.ordinal < dispatch.ordinal
        && dispatch.ordinal < apply.ordinal
        && apply.ordinal < capture.ordinal
        && capture.ordinal < qualify.ordinal
        && qualify.ordinal < apply_upload.ordinal
        && apply_upload.ordinal < apply_finalizer.ordinal)
    {
        errors.push(format!(
            "{name} must recover before new work and retain durable evidence before finalization."
        ));
    }
    for job in ["prepare", "apply"] {
        let held = steps.iter().find(|step| {
            step.job == job
                && step.raw.get("if").and_then(YamlValue::as_str)
                    == Some("steps.recovery.outputs.outcome == 'recovery_needed'")
                && step.run.contains("exit 1")
        });
        let diagnostic = steps.iter().any(|step| {
            step.job == job
                && step.uses.starts_with("actions/upload-artifact@")
                && step.raw.get("if").and_then(YamlValue::as_str)
                    == Some("always() && steps.recovery.outputs.outcome == 'recovery_needed'")
                && step
                    .raw
                    .get("with")
                    .and_then(|w| w.get("name"))
                    .and_then(YamlValue::as_str)
                    .is_some_and(|name| {
                        name.starts_with("execution-state-") && name.contains("recovery-needed-")
                    })
        });
        if held.is_none() || !diagnostic {
            errors.push(format!(
                "{name} {job} must stop and separately retain uncertain recovery diagnostics."
            ));
        }
    }
    if !prepare_recovery
        .run
        .contains("--recovery-key workflow-history-v2")
        || !preview.run.contains("previews/execution.json")
        || !preview.run.contains("plan extract")
        || steps
            .iter()
            .any(|step| step.job == "prepare" && step.run.contains("runs context-record-plan"))
        || !noop_context
            .run
            .contains("execution-state-adapter prepare-noop")
        || !noop_context.run.contains("runs context-start")
        || !noop_context.run.contains("runs context-phase")
        || !noop_context.run.contains("--phase prepared")
        || !noop_context
            .run
            .contains("--github-output \"$GITHUB_OUTPUT\"")
        || !finish_noop.run.contains("runs finish-noop")
        || !finish_noop.run.contains("--workflow-sha")
    {
        errors.push(format!("{name} prepare must retain a signed preview and finish only its plan-free native no-op."));
    }
    if prepare_upload.raw.get("if").and_then(YamlValue::as_str)
        != Some("always() && steps.finish_noop.outcome == 'success'")
        || !handoff_upload
            .raw
            .get("if")
            .and_then(YamlValue::as_str)
            .is_some_and(|v| v.contains("steps.terminal_artifact.outcome == 'success'"))
        || !prepare_finalizer
            .raw
            .get("if")
            .and_then(YamlValue::as_str)
            .is_some_and(|v| {
                v.contains("steps.terminal_artifact.outcome == 'success'")
                    && v.contains("steps.handoff_artifact.outcome == 'success'")
            })
    {
        errors.push(format!("{name} prepare must upload a completed no-op and its transport before guarded checkpointing."));
    }
    for (upload, transport) in [
        (prepare_upload, false),
        (handoff_upload, true),
        (apply_upload, false),
    ] {
        let expected_name = if transport {
            "${{ steps.recovery.outputs.artifact_name }}-handoff-00"
        } else {
            "${{ steps.recovery.outputs.artifact_name }}"
        };
        if !upload.raw.get("with").is_some_and(|w| {
            w.get("name").and_then(YamlValue::as_str) == Some(expected_name)
                && w.get("path").and_then(YamlValue::as_str)
                    == Some("${{ runner.temp }}/execution-state-package")
                && w.get("include-hidden-files").and_then(YamlValue::as_bool) == Some(true)
                && w.get("retention-days").and_then(YamlValue::as_i64)
                    == Some(if transport { 14 } else { 90 })
        }) {
            errors.push(format!("{name} must retain exact native artifact names, private package paths and bounded retention."));
        }
    }
    if !source.run.contains("runs acquire-handoff")
        || !source.run.contains("--purpose transport")
        || !source.run.contains("--artifact-id")
        || !source.run.contains("--artifact-digest")
        || !source.run.contains("--run-id \"$PLAN_RUN_ID\"")
        || source
            .raw
            .get("env")
            .and_then(|v| v.get("PLAN_RUN_ID"))
            .and_then(YamlValue::as_str)
            != Some("${{ inputs.plan_run_id }}")
        || steps
            .iter()
            .any(|step| step.job == "apply" && step.uses.starts_with("actions/download-artifact@"))
    {
        errors.push(format!("{name} apply must acquire an immutable native handoff as transport from the operator-selected preparation run."));
    }
    let binding = |step: &WorkflowStepView, key: &str, expected: &str| {
        step.raw
            .get("env")
            .and_then(|env| env.get(key))
            .and_then(YamlValue::as_str)
            == Some(expected)
    };
    if !apply_inputs
        .run
        .contains("execution-state-adapter validate-apply-inputs")
        || !binding(apply_inputs, "OPERATION", "${{ inputs.operation }}")
        || !binding(apply_inputs, "PLAN_RUN_ID", "${{ inputs.plan_run_id }}")
        || !binding(
            apply_inputs,
            "APPROVED_SHA",
            "${{ inputs.approve_plan_sha }}",
        )
        || !apply_inputs.run.contains("--event \"$GITHUB_EVENT_PATH\"")
        || !apply_inputs
            .run
            .contains("--approve-plan-sha \"$APPROVED_SHA\"")
        || !review.run.contains("execution-state-adapter prepare-apply")
        || !binding(review, "APPROVED_SHA", "${{ inputs.approve_plan_sha }}")
        || !binding(review, "PLAN_RUN_ID", "${{ inputs.plan_run_id }}")
        || !binding(
            review,
            "SOURCE_PACKAGE",
            "${{ steps.source_plan.outputs.package_root }}",
        )
        || !review
            .run
            .contains("--current-event \"$GITHUB_EVENT_PATH\"")
        || !review.run.contains("--approve-plan-sha \"$APPROVED_SHA\"")
        || review.raw.get("if").and_then(YamlValue::as_str)
            != Some("steps.source_plan.outcome == 'success'")
    {
        errors.push(format!("{name} must bind the current operator-selected run, exact approval and raw dispatch before registering a plan."));
    }
    if !register.run.contains("runs context-start")
        || !register.run.contains("runs context-record-plan")
        || !register.run.contains("--input \"native-plan=")
        || !register
            .run
            .contains("--review-path reviews/execution.json")
        || !register.run.contains("--review-sha256")
        || register.run.contains("runs context-phase")
        || register.raw.get("if").and_then(YamlValue::as_str)
            != Some("steps.review_context.outcome == 'success'")
    {
        errors.push(format!("{name} must register original signed plan bytes and the current dispatch approval before applying."));
    }
    if restored.raw.get("if").and_then(YamlValue::as_str)
        != Some("steps.recovery.outputs.outcome == 'resumed'")
        || !restored
            .run
            .contains("execution-state-adapter check-resumed-apply")
        || !binding(restored, "APPROVED_SHA", "${{ inputs.approve_plan_sha }}")
        || !binding(restored, "PLAN_RUN_ID", "${{ inputs.plan_run_id }}")
        || !install_journal
            .run
            .contains("[[ -s \"$root/recovery-source.json\" ]]")
        || install_journal.raw.get("if").and_then(YamlValue::as_str)
            != Some(
                "steps.resumed_plan.outputs.authorization_matches == 'true' && steps.resumed_plan.outputs.journal_action == 'install'",
            )
    {
        errors.push(format!("{name} may install a journal only for a matching resumed package with its exact recovery source."));
    }
    let authorization = "always() && (steps.fresh_context.outputs.authorization_matches == 'true' || steps.resumed_plan.outputs.authorization_matches == 'true')";
    if qualify.raw.get("if").and_then(YamlValue::as_str) != Some(authorization)
        || apply_upload.raw.get("if").and_then(YamlValue::as_str) != Some(authorization)
        || !steps.iter().any(|step| {
            step.job == "apply"
                && step.run.contains("exit 1")
                && step
                    .raw
                    .get("if")
                    .and_then(YamlValue::as_str)
                    .is_some_and(|v| {
                        v.contains("steps.resumed_plan.outputs.authorization_matches != 'true'")
                    })
        })
    {
        errors.push(format!("{name} must retain mismatched approval inputs as a hold without qualification or success settlement."));
    }
    if !dispatch
        .run
        .contains("--name execution --status dispatching")
        || apply.raw.get("if").and_then(YamlValue::as_str)
            != Some("steps.mark_dispatching.outcome == 'success'")
        || apply
            .raw
            .get("env")
            .and_then(|v| v.get("APPROVED_SHA"))
            .and_then(YamlValue::as_str)
            != Some("${{ inputs.approve_plan_sha }}")
        || !apply.run.contains("--approve-plan-sha \"$APPROVED_SHA\"")
        || !apply.run.contains("apply-results/execution.json")
        || !capture.run.contains("journal-root")
        || !capture.run.contains("--name execution --status completed")
        || !capture
            .run
            .contains(".data.status == \"completed\" and .data.plan_sha256 == $sha")
    {
        errors.push(format!("{name} must preserve native dispatch intent, durable journal capture and exact typed completion."));
    }
    for finalizer in [prepare_finalizer, apply_finalizer] {
        if !finalizer.run.contains("finalize-gh-steward-run")
            || !finalizer.run.contains("--artifact-id")
            || !finalizer.run.contains("--artifact-digest")
        {
            errors.push(format!(
                "{name} must finalize only the retained immutable artifact ID and digest."
            ));
        }
    }
    if !text.contains("steps.finalize_checkpoint.outputs.outcome == 'checkpoint'") {
        errors.push(format!(
            "{name} must retain the native settlement checkpoint."
        ));
    }
}
