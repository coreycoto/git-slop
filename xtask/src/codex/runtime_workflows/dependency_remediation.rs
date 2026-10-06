use super::*;

pub(super) fn validate_dependency_remediation_trust(
    text: &str,
    payload: &YamlValue,
    steps: &[WorkflowStepView],
    errors: &mut Vec<String>,
) {
    let name = "dependency-remediation.yml";
    if !text.contains("github.actor == 'dependabot[bot]'") {
        errors.push(format!(
            "{name} must restrict pull_request_target execution to Dependabot."
        ));
    }
    let Some(jobs) = payload.get("jobs").and_then(YamlValue::as_mapping) else {
        errors.push(format!(
            "{name} must isolate proposal, candidate verification, and trusted publication jobs."
        ));
        return;
    };
    let job = |job_name: &str| jobs.get(YamlValue::String(job_name.to_owned()));
    for job_name in [
        "capture",
        "proposal",
        "test-candidate",
        "verify-candidate",
        "publish",
    ] {
        let Some(job) = job(job_name) else {
            errors.push(format!("{name} must define the {job_name} stage."));
            return;
        };
        let permissions = job.get("permissions").and_then(YamlValue::as_mapping);
        let expected: &[(&str, &str)] = match job_name {
            "capture" => &[("contents", "read")],
            "proposal" | "test-candidate" | "verify-candidate" => {
                &[("actions", "read"), ("contents", "read")]
            }
            "publish" => &[("contents", "read")],
            _ => unreachable!(),
        };
        if permissions.is_none() {
            errors.push(format!(
                "{name} {job_name} job must declare least-privilege permissions."
            ));
            continue;
        }
        let permissions = permissions.unwrap();
        for (scope, expected_value) in expected {
            if permissions
                .get(YamlValue::String((*scope).to_owned()))
                .and_then(YamlValue::as_str)
                != Some(*expected_value)
            {
                errors.push(format!(
                    "{name} {job_name} job must grant {scope}: {expected_value}."
                ));
            }
        }
        if job.get("env").is_some_and(|env| {
            yaml_mapping_has_key(env, "GH_TOKEN")
                || yaml_mapping_has_key(env, "GITHUB_TOKEN")
                || yaml_contains(env, "secrets.")
        }) {
            errors.push(format!(
                "{name} {job_name} job must not expose credentials through job environment."
            ));
        }
        if permissions
            .values()
            .any(|permission| matches!(permission.as_str(), Some("write" | "admin")))
        {
            errors.push(format!("{name} {job_name} job must remain read-only."));
        }
    }

    let steps_for = |job_name: &str| {
        steps
            .iter()
            .filter(|step| step.job == job_name)
            .collect::<Vec<_>>()
    };
    let capture_steps = steps_for("capture");
    let proposal_steps = steps_for("proposal");
    let test_steps = steps_for("test-candidate");
    let verify_steps = steps_for("verify-candidate");
    let publish_steps = steps_for("publish");
    let trusted_ref = concat!("$", "{{ github.workflow_sha }}");
    let trusted_only = |job_steps: &[&WorkflowStepView]| {
        job_steps
            .iter()
            .filter(|step| step.uses.starts_with("actions/checkout@"))
            .all(|step| step.checkout_ref == trusted_ref && step.persist_credentials == Some(false))
    };
    let has_trusted_checkout = |job_steps: &[&WorkflowStepView]| {
        job_steps.iter().any(|step| {
            step.uses.starts_with("actions/checkout@")
                && step.checkout_ref == trusted_ref
                && step.persist_credentials == Some(false)
        })
    };
    let credentials_disabled = |job_steps: &[&WorkflowStepView]| {
        job_steps
            .iter()
            .filter(|step| step.uses.starts_with("actions/checkout@"))
            .all(|step| step.persist_credentials == Some(false))
    };

    if !trusted_only(&proposal_steps)
        || proposal_steps
            .iter()
            .filter(|step| step.uses.starts_with("actions/checkout@"))
            .count()
            != 1
    {
        errors.push(format!(
            "{name} proposal must use only the exact trusted workflow checkout with credentials disabled."
        ));
    }
    let codex = proposal_steps.iter().find(|step| step.uses == CODEX_ACTION);
    if codex.is_none_or(|step| {
        step.raw.get("env").is_some_and(|env| {
            yaml_mapping_has_key(env, "GH_TOKEN")
                || yaml_mapping_has_key(env, "GITHUB_TOKEN")
                || yaml_contains(env, "secrets.GITHUB_TOKEN")
        })
    }) {
        errors.push(format!(
            "{name} bounded Codex proposal must run without a GitHub token."
        ));
    }
    if proposal_steps.iter().any(|step| {
        step.uses.starts_with("actions/checkout@")
            && (step.checkout_ref.contains("pull_request.head")
                || step.checkout_ref == concat!("$", "{{ github.sha }}"))
    }) || proposal_steps.iter().any(|step| {
        step.run.contains("cargo test")
            || step.run.contains("dependency-remediation-publish.sh")
            || step.run.contains("working-directory: source")
    }) {
        errors.push(format!(
            "{name} Codex proposal must not check out or execute dependency-head code."
        ));
    }
    for required in [
        "cp .github/codex/prompts/dependency-remediation.md",
        "cp .github/codex/schemas/dependency-remediation.json",
        "scripts/prepare-codex-plugins.sh",
        "dependency-remediation collect-proposal",
    ] {
        if !text.contains(required) {
            errors.push(format!(
                "{name} must provide bounded trusted proposal input {required}."
            ));
        }
    }

    let source_ref = concat!("$", "{{ steps.source.outputs.source_sha }}");
    let candidate_ref = concat!("$", "{{ steps.validated_source.outputs.source_sha }}");
    let source_checkout = capture_steps.iter().any(|step| {
        step.uses.starts_with("actions/checkout@")
            && step.checkout_ref == source_ref
            && step.persist_credentials == Some(false)
            && step
                .raw
                .get("with")
                .is_some_and(|with| with.get("path").and_then(YamlValue::as_str) == Some("source"))
    });
    let candidate_checkout = verify_steps.iter().any(|step| {
        step.uses.starts_with("actions/checkout@")
            && step.checkout_ref == candidate_ref
            && step.persist_credentials == Some(false)
            && step.raw.get("with").is_some_and(|with| {
                with.get("path").and_then(YamlValue::as_str) == Some("candidate")
            })
    });
    if !has_trusted_checkout(&capture_steps)
        || !source_checkout
        || !credentials_disabled(&capture_steps)
        || !has_trusted_checkout(&verify_steps)
        || !candidate_checkout
        || !credentials_disabled(&verify_steps)
    {
        errors.push(format!(
            "{name} capture and candidate verification must use exact event-bound source checkouts without persisted credentials."
        ));
    }
    let source_build =
        capture_steps.iter().find(|step| {
            step.run.contains(
            "cargo build --manifest-path \"$GITHUB_WORKSPACE/xtask/Cargo.toml\" --locked --release",
        ) && step.run.contains("GIT_SLOP_XTASK_BIN=%s/release/git-slop-xtask")
        });
    let source_tests = capture_steps
        .iter()
        .find(|step| step.run.contains("dependency-remediation verify-source"));
    if source_build
        .is_none_or(|build| source_tests.is_none_or(|test| build.ordinal >= test.ordinal))
        || source_tests.is_none()
        || !test_steps.iter().any(|step| {
            step.run
                .contains("env -u GH_TOKEN -u GITHUB_TOKEN -u OPENAI_API_KEY cargo test")
        })
    {
        errors.push(format!(
            "{name} must use a prebuilt trusted adapter to verify source and test the materialized candidate in credential-free jobs."
        ));
    }
    let trusted_xtask = verify_steps.iter().find(|step| {
        step.run.contains(
            "cargo build --manifest-path \"$GITHUB_WORKSPACE/xtask/Cargo.toml\" --locked --release",
        ) && step
            .run
            .contains("GIT_SLOP_XTASK_BIN=%s/release/git-slop-xtask")
    });
    let candidate_apply = verify_steps.iter().find(|step| {
        step.run.contains("$GIT_SLOP_XTASK_BIN")
            && step.run.contains("dependency-remediation apply-candidate")
    });
    let candidate_check = verify_steps.iter().find(|step| {
        step.run.contains("$GIT_SLOP_XTASK_BIN")
            && step
                .run
                .contains("dependency-remediation verify-candidate \"$SOURCE_SHA\"")
            && step.run.contains("candidate_tree_sha")
    });
    if trusted_xtask.is_none_or(|build| {
        candidate_apply.is_none_or(|apply| build.ordinal >= apply.ordinal)
            || candidate_check.is_none_or(|check| build.ordinal >= check.ordinal)
    }) || candidate_apply.is_none()
        || candidate_check.is_none()
    {
        errors.push(format!(
            "{name} must use a prebuilt trusted xtask to apply and recheck the exact source-bound candidate tree."
        ));
    }

    validate_dependency_candidate_custody(payload, &test_steps, &verify_steps, errors);

    let publish_job = job("publish");
    let publish_condition = publish_job
        .and_then(|job| job.get("if"))
        .and_then(YamlValue::as_str)
        .unwrap_or_default();
    let recovered_hold = publish_steps.iter().find(|step| {
        step.raw.get("name").and_then(YamlValue::as_str)
            == Some("Preserve the exact recovered publication hold")
    });
    if !publish_condition.contains("needs.recover.outputs.outcome == 'resumed'")
        || publish_steps.len() != 1
        || recovered_hold.is_none()
        || publish_steps.iter().any(|step| {
            step.uses.starts_with("actions/checkout@")
                || step.uses.starts_with("actions/upload-artifact@")
                || step.run.contains("dependency-remediation-publish.sh")
                || step.run.contains("dependency-remediation publish")
                || step.run.contains("runs acquire-publication-candidate")
                || step.run.contains("runs verify-publication")
                || step.run.contains("git push")
                || step.run.contains("gh pr create")
                || step.raw.get("env").is_some_and(|env| {
                    yaml_mapping_has_key(env, "GH_TOKEN")
                        || yaml_mapping_has_key(env, "GITHUB_TOKEN")
                        || yaml_contains(env, "secrets.")
                })
        })
    {
        errors.push(format!(
            "{name} must preserve recovered publication holds without invoking Rust publication, GitHub writes, or privileged credentials."
        ));
    }
    if publish_job.is_none_or(|publish| {
        publish
            .get("needs")
            .is_none_or(|needs| !yaml_contains(needs, "recover"))
    }) {
        errors.push(format!(
            "{name} recovered publication hold must be controlled by authoritative gh-steward recovery."
        ));
    }
    let settle_job = job("settle-noop");
    let settle_condition = settle_job
        .and_then(|job| job.get("if"))
        .and_then(YamlValue::as_str)
        .unwrap_or_default();
    if !settle_condition.contains("needs.recover.outputs.outcome == 'fresh'")
        || !settle_condition.contains("needs.publish.result == 'skipped'")
        || !settle_condition.contains("needs.proposal.outputs.ready != 'true'")
        || !settle_condition.contains("needs.test-candidate.result == 'success'")
        || !settle_condition.contains("needs.verify-candidate.result == 'success'")
        || settle_job.is_none_or(|job| {
            job.get("needs").is_none_or(|needs| {
                ["test-candidate", "verify-candidate", "publish"]
                    .iter()
                    .any(|needed| !yaml_contains(needs, needed))
            })
        })
        || !text.contains("needs.proposal.outputs.noop_reason || 'prerequisite-unavailable'")
    {
        errors.push(format!(
            "{name} must settle only fresh no-publication outcomes, including a tested candidate whose publisher is deliberately disabled."
        ));
    }
    for required in [
        "publication/candidate.json",
        "publication/patch.diff",
        "publication/result.json",
        "events/trigger-event.json",
        "candidate_tree_sha",
    ] {
        if !text.contains(required) {
            errors.push(format!(
                "{name} must bind exact candidate files and credential-free verification evidence."
            ));
            break;
        }
    }
    if !verify_steps.iter().any(|step| {
        step.run
            .contains("dependency-remediation create-candidate-evidence")
    }) {
        errors.push(format!(
            "{name} candidate evidence must be created by the trusted native adapter."
        ));
    }

    let noop_prepare = steps.iter().find(|step| {
        step.job == "settle-noop" && step.run.contains("dependency-remediation prepare-noop")
    });
    let noop_context_start = steps
        .iter()
        .find(|step| step.job == "settle-noop" && step.run.contains("runs context-start"));
    let noop_context_phase = steps
        .iter()
        .find(|step| step.job == "settle-noop" && step.run.contains("runs context-phase"));
    let noop_finish = steps
        .iter()
        .find(|step| step.job == "settle-noop" && step.run.contains("runs finish-noop"));
    if noop_prepare.is_none_or(|prepare| {
        prepare.raw.get("env").is_some_and(|env| {
            yaml_mapping_has_key(env, "GH_TOKEN")
                || yaml_mapping_has_key(env, "GITHUB_TOKEN")
                || yaml_contains(env, "secrets.")
        }) || noop_context_start.is_none_or(|start| prepare.ordinal >= start.ordinal)
    }) || noop_context_phase
        .is_none_or(|phase| noop_context_start.is_none_or(|start| start.ordinal >= phase.ordinal))
        || noop_finish.is_none_or(|finish| {
            noop_context_phase.is_none_or(|phase| phase.ordinal >= finish.ordinal)
        })
    {
        errors.push(format!(
            "{name} must prepare exact no-op inputs without credentials before gh-steward records and settles the terminal decision."
        ));
    }
}

fn validate_dependency_candidate_custody(
    payload: &YamlValue,
    test_steps: &[&WorkflowStepView],
    verify_steps: &[&WorkflowStepView],
    errors: &mut Vec<String>,
) {
    let name = "dependency-remediation.yml";
    let jobs = payload.get("jobs").and_then(YamlValue::as_mapping);
    let candidate_test_job =
        jobs.and_then(|jobs| jobs.get(YamlValue::String("test-candidate".into())));
    let candidate_verify_job =
        jobs.and_then(|jobs| jobs.get(YamlValue::String("verify-candidate".into())));
    let test_step = test_steps.iter().find(|step| {
        step.run.contains("dependency-remediation apply-candidate")
            && step
                .run
                .contains("env -u GH_TOKEN -u GITHUB_TOKEN -u OPENAI_API_KEY cargo test")
    });
    let test_permissions = candidate_test_job
        .and_then(|job| job.get("permissions"))
        .and_then(YamlValue::as_mapping);
    let test_needs = candidate_test_job.and_then(|job| job.get("needs"));
    let last_test_step = test_steps.iter().max_by_key(|step| step.ordinal);
    if candidate_test_job.is_none_or(|job| job.get("outputs").is_some())
        || test_step.is_none()
        || test_step.is_none_or(|step| {
            step.run.find("dependency-remediation apply-candidate")
                >= step.run.find("cargo test -p git-slop")
        })
        || last_test_step
            .is_none_or(|last| test_step.is_none_or(|test| last.ordinal != test.ordinal))
        || test_steps
            .iter()
            .any(|step| step.uses.starts_with("actions/upload-artifact@"))
        || test_permissions.is_none_or(|permissions| {
            permissions
                .values()
                .any(|value| matches!(value.as_str(), Some("write" | "admin")))
        })
        || ["actions", "contents"].iter().any(|scope| {
            test_permissions
                .and_then(|permissions| permissions.get(YamlValue::String((*scope).into())))
                .and_then(YamlValue::as_str)
                != Some("read")
        })
        || !yaml_contains(test_needs.unwrap_or(&YamlValue::Null), "capture")
        || !yaml_contains(test_needs.unwrap_or(&YamlValue::Null), "proposal")
    {
        errors.push(format!(
            "{name} must stop the read-only test job after scrubbed tests and never emit trusted evidence or artifacts from that runner."
        ));
    }

    let verify_needs = candidate_verify_job.and_then(|job| job.get("needs"));
    let verify_name = candidate_verify_job
        .and_then(|job| job.get("name"))
        .and_then(YamlValue::as_str);
    let verify_apply = verify_steps
        .iter()
        .find(|step| step.run.contains("dependency-remediation apply-candidate"));
    let verify_evidence = verify_steps.iter().find(|step| {
        step.run
            .contains("dependency-remediation create-candidate-evidence")
    });
    let verify_upload = verify_steps
        .iter()
        .find(|step| step.uses.starts_with("actions/upload-artifact@"));
    if verify_name != Some("Verify publication candidate")
        || !yaml_contains(verify_needs.unwrap_or(&YamlValue::Null), "test-candidate")
        || !yaml_contains(verify_needs.unwrap_or(&YamlValue::Null), "capture")
        || !yaml_contains(verify_needs.unwrap_or(&YamlValue::Null), "proposal")
        || verify_steps
            .iter()
            .any(|step| step.run.contains("cargo test"))
        || verify_apply.is_none_or(|apply| {
            verify_evidence.is_none_or(|evidence| apply.ordinal >= evidence.ordinal)
                || verify_upload.is_none_or(|upload| apply.ordinal >= upload.ordinal)
        })
        || verify_evidence.is_none_or(|evidence| {
            verify_upload.is_none_or(|upload| evidence.ordinal >= upload.ordinal)
        })
        || verify_steps
            .iter()
            .map(|step| {
                step.run
                    .matches("dependency-remediation verify-artifact")
                    .count()
            })
            .sum::<usize>()
            < 2
    {
        errors.push(format!(
            "{name} must use the gh-steward-required unique job name Verify publication candidate in a fresh trusted job to consume the test job result, revalidate both exact input artifacts, reapply the candidate and create its sole publication evidence."
        ));
    }
}

pub fn validate_dependency_candidate_artifact_ids(source: &str, errors: &mut Vec<String>) {
    for required in [
        "let id = positive_env(&format!(\"{prefix}_ID\"))?;",
        "Ok(json!({\"id\":id,\"name\":name,\"digest\":digest}))",
    ] {
        if !source.contains(required) {
            errors.push(
                "dependency-remediation candidate evidence must preserve upstream artifact IDs as positive JSON integers."
                    .to_owned(),
            );
            return;
        }
    }
}
