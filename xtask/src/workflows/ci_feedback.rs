fn validate_ci_feedback_contract(workflows: &Path, errors: &mut Vec<String>) {
    const GROUP: &str = "ci-${{ github.event.pull_request.number || github.ref }}";
    let Some(main_text) = read(&workflows.join("ci.yml"), errors) else {
        return;
    };
    let Some(main) = parse_ci_workflow(&main_text, "ci.yml", errors) else {
        return;
    };
    let concurrency = main.get("concurrency");
    if concurrency
        .and_then(|value| value.get("group"))
        .and_then(YamlValue::as_str)
        != Some(GROUP)
    {
        errors.push(
            "ci.yml must use the pull-request-aware concurrency group for superseded work."
                .into(),
        );
    }
    if concurrency
        .and_then(|value| value.get("cancel-in-progress"))
        .and_then(YamlValue::as_bool)
        != Some(true)
    {
        errors.push("ci.yml must set concurrency.cancel-in-progress to true.".into());
    }

    for (name, job_names) in [
        ("ci.yml", &["full-validation"][..]),
        (
            "ci-public.yml",
            &["public-rust", "action-tests", "msrv", "platform-smoke"][..],
        ),
        (
            "ci-maintainer.yml",
            &[
                "workflow-lint",
                "change-classification",
                "maintainer-contracts",
                "supply-chain",
            ][..],
        ),
    ] {
        let text = if name == "ci.yml" {
            main_text.clone()
        } else {
            let Some(text) = read(&workflows.join(name), errors) else {
                continue;
            };
            text
        };
        let Some(payload) = parse_ci_workflow(&text, name, errors) else {
            continue;
        };
        let Some(jobs) = payload.get("jobs").and_then(YamlValue::as_mapping) else {
            errors.push(format!("{name} must define jobs."));
            continue;
        };
        for job_name in job_names {
            validate_timed_ci_job(jobs, name, job_name, errors);
        }
    }
}

fn parse_ci_workflow(text: &str, name: &str, errors: &mut Vec<String>) -> Option<YamlValue> {
    match serde_yaml::from_str(text) {
        Ok(payload) => Some(payload),
        Err(error) => {
            errors.push(format!("Unable to parse {name}: {error}"));
            None
        }
    }
}

fn validate_timed_ci_job(
    jobs: &serde_yaml::Mapping,
    name: &str,
    job_name: &str,
    errors: &mut Vec<String>,
) {
    let Some(job) = jobs.get(YamlValue::String(job_name.to_string())) else {
        errors.push(format!("{name} must define the {job_name} feedback lane."));
        return;
    };
    if !job
        .get("timeout-minutes")
        .and_then(YamlValue::as_u64)
        .is_some_and(|minutes| minutes > 0)
    {
        errors.push(format!(
            "{name} {job_name} must set a positive timeout-minutes value."
        ));
    }
    let steps = job
        .get("steps")
        .and_then(YamlValue::as_sequence)
        .map(Vec::as_slice)
        .unwrap_or_default();
    for required_step in ["Start lane timer", "Report lane timing"] {
        if !steps
            .iter()
            .any(|step| step.get("name").and_then(YamlValue::as_str) == Some(required_step))
        {
            errors.push(format!(
                "{name} {job_name} must include the {required_step} step."
            ));
        }
    }
}
