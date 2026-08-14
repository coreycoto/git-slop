use serde_yaml::{Mapping, Value as YamlValue};

use super::{job, require, require_needs, step_run};

pub(super) fn validate_release_canary(jobs: &Mapping, name: &str, errors: &mut Vec<String>) {
    let Some(canary) = job(jobs, "exercise-remediation-canary", name, errors) else {
        return;
    };
    require_needs(
        canary,
        name,
        "exercise-remediation-canary",
        &["verify-publication"],
        errors,
    );
    if canary
        .get("permissions")
        .and_then(|permissions| permissions.get("actions"))
        .and_then(YamlValue::as_str)
        != Some("write")
    {
        errors.push(format!(
            "{name} exercise-remediation-canary must receive only the Actions write permission needed to dispatch its read-only canary."
        ));
    }
    let Some(run) = step_run(canary, "Dispatch read-only release closeout canary") else {
        errors.push(format!(
            "{name} must dispatch the dependency-remediation canary."
        ));
        return;
    };
    for required in [
        "gh workflow run dependency-remediation.yml",
        "--repo \"$GITHUB_REPOSITORY\"",
        "--ref main",
        "--field mode=canary",
        "--field release_tag=\"$TAG\"",
    ] {
        require(run, required, name, errors);
    }
}
