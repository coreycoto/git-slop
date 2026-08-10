fn validate_exact_release_assets(run: &str, name: &str, tag: &str, errors: &mut Vec<String>) {
    let normalized = run
        .split_whitespace()
        .filter(|token| *token != "\\")
        .collect::<Vec<_>>()
        .join(" ");
    let manifest_path = if name == "homebrew-handoff.yml" {
        "release-assets/release-manifest.json"
    } else if name == "release-published.yml" {
        "verification/release-manifest.json"
    } else {
        "release-manifest.inventory.json"
    };
    let manifest_inventory = format!(
        "jq -r '.artifacts[].name, .supplemental_assets[].name' {manifest_path} > expected-assets.txt"
    );
    if !normalized.contains(&manifest_inventory) {
        errors.push(format!(
            "{name} must derive the required release-asset inventory from manifest roles."
        ));
    }
    for required in [
        "printf '%s\\n' SHA256SUMS release-manifest.json >> expected-assets.txt",
        "LC_ALL=C sort -o expected-assets.txt expected-assets.txt",
        "actual-assets.txt",
        "diff -u expected-assets.txt actual-assets.txt",
    ] {
        require(&normalized, required, name, errors);
    }
    let _ = tag;
}

fn workflow_steps(payload: &YamlValue) -> Vec<&YamlValue> {
    payload
        .get("jobs")
        .and_then(YamlValue::as_mapping)
        .into_iter()
        .flat_map(|jobs| jobs.values())
        .flat_map(steps)
        .collect()
}

fn env_has_key(value: &YamlValue, key: &str) -> bool {
    value
        .get("env")
        .and_then(YamlValue::as_mapping)
        .is_some_and(|env| env.contains_key(YamlValue::String(key.to_owned())))
}

fn workflow_or_job_env_contains(payload: &YamlValue, key: &str) -> bool {
    env_has_key(payload, key)
        || payload
            .get("jobs")
            .and_then(YamlValue::as_mapping)
            .is_some_and(|jobs| jobs.values().any(|job| env_has_key(job, key)))
}

fn workflow_or_job_env_contains_value(payload: &YamlValue, expected: &str) -> bool {
    let contains = |value: &YamlValue| {
        value
            .get("env")
            .and_then(YamlValue::as_mapping)
            .is_some_and(|env| env.values().any(|value| value.as_str() == Some(expected)))
    };
    contains(payload)
        || payload
            .get("jobs")
            .and_then(YamlValue::as_mapping)
            .is_some_and(|jobs| jobs.values().any(contains))
}

fn yaml_string_occurrences(value: &YamlValue, expected: &str) -> usize {
    match value {
        YamlValue::String(value) => value.matches(expected).count(),
        YamlValue::Sequence(values) => values
            .iter()
            .map(|value| yaml_string_occurrences(value, expected))
            .sum(),
        YamlValue::Mapping(values) => values
            .values()
            .map(|value| yaml_string_occurrences(value, expected))
            .sum(),
        _ => 0,
    }
}

fn require_exact_trigger(
    payload: &YamlValue,
    name: &str,
    expected: &str,
    errors: &mut Vec<String>,
) {
    let keys = payload
        .get("on")
        .and_then(YamlValue::as_mapping)
        .map(mapping_keys)
        .unwrap_or_default();
    if keys != BTreeSet::from([expected.to_owned()]) {
        errors.push(format!(
            "{name} must define only the {expected} trigger; found {}.",
            keys.into_iter().collect::<Vec<_>>().join(", ")
        ));
    }
}

fn require_exact_job_set(
    jobs: &serde_yaml::Mapping,
    name: &str,
    expected: &[&str],
    errors: &mut Vec<String>,
) {
    let actual = mapping_keys(jobs);
    let expected = expected
        .iter()
        .map(|value| (*value).to_owned())
        .collect::<BTreeSet<_>>();
    if actual != expected {
        errors.push(format!(
            "{name} job graph does not match the release contract."
        ));
    }
}

fn mapping_keys(mapping: &serde_yaml::Mapping) -> BTreeSet<String> {
    mapping
        .keys()
        .filter_map(YamlValue::as_str)
        .map(str::to_owned)
        .collect()
}

fn job<'a>(
    jobs: &'a serde_yaml::Mapping,
    key: &str,
    name: &str,
    errors: &mut Vec<String>,
) -> Option<&'a YamlValue> {
    match jobs.get(YamlValue::String(key.to_owned())) {
        Some(job) => Some(job),
        None => {
            errors.push(format!("{name} must define job {key}."));
            None
        }
    }
}

fn steps(job: &YamlValue) -> Vec<&YamlValue> {
    job.get("steps")
        .and_then(YamlValue::as_sequence)
        .map(|steps| steps.iter().collect())
        .unwrap_or_default()
}

fn named_step<'a>(job: &'a YamlValue, step_name: &str) -> Option<&'a YamlValue> {
    steps(job)
        .into_iter()
        .find(|step| step.get("name").and_then(YamlValue::as_str) == Some(step_name))
}

fn step_env<'a>(step: &'a YamlValue, key: &str) -> Option<&'a str> {
    step.get("env")
        .and_then(|env| env.get(key))
        .and_then(YamlValue::as_str)
}

fn step_run<'a>(job: &'a YamlValue, step_name: &str) -> Option<&'a str> {
    named_step(job, step_name).and_then(|step| step.get("run").and_then(YamlValue::as_str))
}

fn require_needs(
    job: &YamlValue,
    name: &str,
    job_name: &str,
    expected: &[&str],
    errors: &mut Vec<String>,
) {
    let actual = match job.get("needs") {
        Some(YamlValue::String(value)) => BTreeSet::from([value.to_owned()]),
        Some(YamlValue::Sequence(values)) => values
            .iter()
            .filter_map(YamlValue::as_str)
            .map(str::to_owned)
            .collect(),
        _ => BTreeSet::new(),
    };
    let expected = expected
        .iter()
        .map(|value| (*value).to_owned())
        .collect::<BTreeSet<_>>();
    if actual != expected {
        errors.push(format!(
            "{name} {job_name} needs do not match the protected release order."
        ));
    }
}

fn require_environment(
    job: &YamlValue,
    name: &str,
    job_name: &str,
    expected: &str,
    errors: &mut Vec<String>,
) {
    if job.get("environment").and_then(YamlValue::as_str) != Some(expected) {
        errors.push(format!(
            "{name} {job_name} must use the required {expected} environment."
        ));
    }
}

fn require_exact_job_permission(
    job: &YamlValue,
    name: &str,
    job_name: &str,
    permission: &str,
    expected: &str,
    errors: &mut Vec<String>,
) {
    let Some(permissions) = job.get("permissions").and_then(YamlValue::as_mapping) else {
        errors.push(format!(
            "{name} {job_name} must grant only {permission}: {expected}."
        ));
        return;
    };
    if permissions.len() != 1
        || permissions.get(permission).and_then(YamlValue::as_str) != Some(expected)
    {
        errors.push(format!(
            "{name} {job_name} must grant only {permission}: {expected}."
        ));
    }
}

fn require_permission(
    payload: &YamlValue,
    name: &str,
    permission: &str,
    expected: &str,
    errors: &mut Vec<String>,
) {
    if payload
        .get("permissions")
        .and_then(|permissions| permissions.get(permission))
        .and_then(YamlValue::as_str)
        != Some(expected)
    {
        errors.push(format!(
            "{name} must grant {permission}: {expected} at workflow scope."
        ));
    }
}

fn validate_target_matrix(
    job: &YamlValue,
    name: &str,
    job_name: &str,
    require_archive: bool,
    errors: &mut Vec<String>,
) {
    let includes = job
        .get("strategy")
        .and_then(|strategy| strategy.get("matrix"))
        .and_then(|matrix| matrix.get("include"))
        .and_then(YamlValue::as_sequence);
    let Some(includes) = includes else {
        errors.push(format!(
            "{name} {job_name} must define an exact target matrix."
        ));
        return;
    };
    let targets = includes
        .iter()
        .filter_map(|entry| entry.get("target").and_then(YamlValue::as_str))
        .collect::<BTreeSet<_>>();
    if includes.len() != RELEASE_TARGETS.len()
        || targets != RELEASE_TARGETS.into_iter().collect::<BTreeSet<_>>()
    {
        errors.push(format!(
            "{name} {job_name} must contain exactly the seven supported targets."
        ));
    }
    let runner_targets = includes
        .iter()
        .filter_map(|entry| Some((entry.get("os")?.as_str()?, entry.get("target")?.as_str()?)))
        .collect::<BTreeSet<_>>();
    if runner_targets != RELEASE_TARGET_RUNNERS.into_iter().collect::<BTreeSet<_>>() {
        errors.push(format!(
            "{name} {job_name} must bind each supported target to its exact runner."
        ));
    }
    if job.get("runs-on").and_then(YamlValue::as_str) != Some("${{ matrix.os }}") {
        errors.push(format!(
            "{name} {job_name} must run each target on matrix.os."
        ));
    }
    for entry in includes {
        if entry.get("os").and_then(YamlValue::as_str).is_none()
            || (require_archive && entry.get("archive").and_then(YamlValue::as_str).is_none())
        {
            errors.push(format!(
                "{name} {job_name} matrix entries must include runner metadata."
            ));
        }
    }
}
