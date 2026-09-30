use super::{CODEX_ACTION, WorkflowStepView};

pub(super) fn validate_args(steps: &[WorkflowStepView], name: &str, errors: &mut Vec<String>) {
    for step in steps.iter().filter(|step| step.uses == CODEX_ACTION) {
        let Some(args) = step
            .raw
            .get("with")
            .and_then(|inputs| inputs.get("codex-args"))
        else {
            continue;
        };
        let parsed = args
            .as_str()
            .and_then(|args| serde_json::from_str::<Vec<String>>(args).ok());
        match parsed {
            Some(args)
                if !args.iter().any(|arg| {
                    arg == "--profile"
                        || arg.starts_with("--profile=")
                        || (arg.starts_with("-p") && !arg.starts_with("--"))
                }) => {}
            _ => errors.push(format!(
                "{name} Codex args must be a JSON string array without profile selectors; \
                 the pinned protected action rejects --profile and -p."
            )),
        }
    }
}
