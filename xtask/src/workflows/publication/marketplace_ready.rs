fn validate_marketplace_ready_job(marketplace_ready: Option<&YamlValue>, errors: &mut Vec<String>) {
    let name = "release-publish.yml";
    if let Some(marketplace_ready) = marketplace_ready {
        if marketplace_ready.get("if").is_some()
            || marketplace_ready
                .get("continue-on-error")
                .is_some_and(|value| value.as_bool() != Some(false))
        {
            errors.push(format!(
                "{name} marketplace-ready must depend normally on successful smoke and fail closed."
            ));
        }
        require_needs(
            marketplace_ready,
            name,
            "marketplace-ready",
            &["publish-crate", "draft-release", "draft-action-smoke"],
            errors,
        );
        let Some(summary) = step_run(marketplace_ready, "Publish Marketplace handoff summary")
        else {
            errors.push(format!(
                "{name} must stop at a Marketplace handoff summary."
            ));
            return;
        };
        if let Some(step) = named_step(marketplace_ready, "Publish Marketplace handoff summary")
            && (step.get("if").is_some()
                || step
                    .get("continue-on-error")
                    .is_some_and(|value| value.as_bool() != Some(false)))
        {
            errors.push(format!(
                "{name} Marketplace handoff summary must execute unconditionally and fail closed."
            ));
        }
        for required in [
            "Marketplace-ready",
            "Open the draft release",
            "only manual approval for the release",
            "already-dispatched Homebrew receiver",
            "existing published release was reverified without mutation, and the dispatch-authorized publication job redispatched",
        ] {
            require(summary, required, name, errors);
        }
    }
}
