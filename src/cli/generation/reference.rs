fn machine_contract(path: &str) -> Option<&'static str> {
    match path {
        "git-slop find" => Some("schema 5 report (`git slop schema report`); `find --estimate-only` uses `find-estimate-1`"),
        "git-slop init" => Some("`init-1` for JSON output"),
        "git-slop show" => Some("`show-1`"),
        "git-slop explain" => Some("`explain-2`"),
        "git-slop plan" => Some("`plan-2`"),
        "git-slop advise" => Some("`advice-input-1` for provider-free context; `advice-1` for release-gated validated advice"),
        "git-slop policy lock" => Some("`policy-lock-1`"),
        "git-slop check" => Some("`check-1`"),
        "git-slop compare" => Some("`compare-1`; NDJSON streaming uses `compare-ndjson-1`"),
        path if path.starts_with("git-slop baseline") => Some("`baseline-1`"),
        path if path.starts_with("git-slop report") => Some("schema 5 report (`git slop schema report`)"),
        "git-slop sarif" => Some("`sarif-1`"),
        "git-slop health" => Some("`health-1` for JSON output"),
        path if path.starts_with("git-slop config") => Some("`config-2`"),
        "git-slop doctor" => Some("`doctor-1`"),
        path if path.starts_with("git-slop list") => Some("`list-1`"),
        "git-slop prune" => Some("`prune-1`"),
        "git-slop cache status" => Some("`cache-status-1`"),
        "git-slop cache prune" => Some("`cache-prune-1`"),
        "git-slop build-info" => Some("`build-info-2`"),
        "git-slop schema" => Some("the selected immutable schema"),
        _ => None,
    }
}

fn command_example(path: &str) -> &'static str {
    match path {
        "git-slop" => "git slop --help",
        "git-slop init" => "git slop init --check",
        "git-slop find" => "git slop find --ephemeral",
        "git-slop show" => "git slop show src/lib.rs",
        "git-slop explain" => "git slop explain --path src/lib.rs",
        "git-slop plan" => "git slop plan --path src/lib.rs",
        "git-slop policy" => "git slop policy list",
        "git-slop policy init" => "git slop policy init ./team-policy",
        "git-slop policy validate" => "git slop policy validate ./team-policy",
        "git-slop policy test" => "git slop policy test ./team-policy",
        "git-slop policy install" => "git slop policy install ./team-policy --select",
        "git-slop policy lock" => "git slop policy lock --format json",
        "git-slop policy list" => "git slop policy list --format json",
        "git-slop policy show" => "git slop policy show core --format json",
        "git-slop policy remove" => "git slop policy remove com.example.team-policy --unselect",
        "git-slop advise" => "git slop advise --top 1",
        "git-slop check" => "git slop check --require-current",
        "git-slop compare" => "git slop compare --baseline main --fail-on-regression",
        "git-slop baseline" => "git slop baseline list",
        "git-slop baseline ensure" => "git slop baseline ensure --name main",
        "git-slop baseline create" => "git slop baseline create --name main",
        "git-slop baseline update" => "git slop baseline update --name main",
        "git-slop baseline list" => "git slop baseline list --format json",
        "git-slop baseline inspect" => "git slop baseline inspect --name main",
        "git-slop baseline validate" => "git slop baseline validate --name main",
        "git-slop baseline remove" => "git slop baseline remove --name main --yes",
        "git-slop report" => "git slop report validate .slop/latest/report.json",
        "git-slop report validate" => "git slop report validate .slop/latest/report.json",
        "git-slop report migrate" => "git slop report migrate old.json --output report.json",
        "git-slop report schema" => "git slop report schema",
        "git-slop sarif" => "git slop sarif --output .slop/latest/findings.sarif",
        "git-slop health" => "git slop health --format markdown --require-current",
        "git-slop config" => "git slop config show --effective",
        "git-slop config show" => "git slop config show --effective",
        "git-slop config validate" => "git slop config validate",
        "git-slop config diff-defaults" => "git slop config diff-defaults",
        "git-slop config migrate" => "git slop config migrate --dry-run",
        "git-slop config schema" => "git slop config schema",
        "git-slop doctor" => "git slop doctor --require-current",
        "git-slop list" => "git slop list interventions --top 20",
        "git-slop list policy-failures" => "git slop list policy-failures --top 20",
        "git-slop list interventions" => "git slop list interventions --top 20",
        "git-slop list observations" => "git slop list observations --top 20",
        "git-slop list health-findings" => "git slop list health-findings --top 20",
        "git-slop list findings" => "git slop list findings --top 20",
        "git-slop list relationships" => "git slop list relationships --top 20",
        "git-slop list clusters" => "git slop list clusters --top 20",
        "git-slop list profiles" => "git slop list profiles --format json",
        "git-slop prune" => "git slop prune --keep 20 --yes",
        "git-slop cache" => "git slop cache status",
        "git-slop cache status" => "git slop cache status --format json",
        "git-slop cache prune" => "git slop cache prune --dry-run",
        "git-slop completions" => "git slop completions bash",
        "git-slop man" => "git slop man --output man/git-slop.1",
        "git-slop reference" => "git slop reference --output docs/cli-reference.md",
        "git-slop html" => "git slop html --output .slop/latest/report.html",
        "git-slop version" => "git slop version",
        "git-slop build-info" => "git slop build-info --format json",
        "git-slop schema" => "git slop schema report",
        _ => "git slop --help",
    }
}

fn markdown_command_body(command: &clap::Command, path: &str, output: &mut String) {
    output.push_str(&format!("## `{path}`\n\n"));
    if let Some(about) = command.get_about() {
        output.push_str(&format!("{about}\n\n"));
    }
    let mut usage_command = command.clone();
    let rendered_usage = usage_command.render_usage().to_string();
    let short_prefix = format!("Usage: {}", command.get_name());
    let full_prefix = format!("Usage: {path}");
    let rendered_usage = rendered_usage.replacen(&short_prefix, &full_prefix, 1);
    output.push_str("**Usage**\n\n```text\n");
    output.push_str(&rendered_usage);
    output.push_str("\n```\n\n");
    if let Some(contract) = machine_contract(path) {
        output.push_str(&format!("**Machine contract:** {contract}.\n\n"));
    }
    let arguments = command
        .get_arguments()
        .filter(|argument| !argument.is_hide_set())
        .collect::<Vec<_>>();
    if !arguments.is_empty() {
        output.push_str("| Argument | Value | Default | Constraints | Description |\n| --- | --- | --- | --- | --- |\n");
        for argument in arguments {
            let name = argument
                .get_long()
                .map(|value| format!("--{value}"))
                .unwrap_or_else(|| argument.get_id().to_string());
            let help = argument.get_help().map(ToString::to_string).unwrap_or_default();
            let takes_values = argument.get_action().takes_values();
            let values = if takes_values {
                argument
                    .get_possible_values()
                    .into_iter()
                    .filter(|value| !value.is_hide_set())
                    .map(|value| value.get_name().to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            } else {
                String::new()
            };
            let value = if takes_values {
                argument
                    .get_value_names()
                    .map(|names| names.iter().map(ToString::to_string).collect::<Vec<_>>().join(" "))
                    .filter(|names| !names.is_empty())
                    .or_else(|| (!values.is_empty()).then(|| values.clone()))
                    .unwrap_or_else(|| "value".to_string())
            } else {
                "flag".to_string()
            };
            let defaults = argument
                .get_default_values()
                .iter()
                .map(|value| value.to_string_lossy())
                .collect::<Vec<_>>()
                .join(", ");
            let mut constraints = Vec::<String>::new();
            if argument.is_required_set() {
                constraints.push("required".to_string());
            }
            if argument.is_global_set() {
                constraints.push("global".to_string());
            }
            if !values.is_empty() {
                constraints.push(format!("values: {values}"));
            }
            let conflicts = command
                .get_arg_conflicts_with(argument)
                .into_iter()
                .map(|conflict| {
                    conflict
                        .get_long()
                        .map_or_else(|| conflict.get_id().to_string(), |name| format!("--{name}"))
                })
                .collect::<std::collections::BTreeSet<_>>();
            if !conflicts.is_empty() {
                constraints.push(format!(
                    "conflicts: {}",
                    conflicts.into_iter().collect::<Vec<_>>().join(", ")
                ));
            }
            for group in command.get_groups() {
                let members = group.get_args().map(ToString::to_string).collect::<Vec<_>>();
                if members.len() < 2 || !members.iter().any(|member| member == argument.get_id()) {
                    continue;
                }
                let mut group = group.clone();
                if !group.is_multiple() {
                    constraints.push(format!("exclusive group: {}", members.join(", ")));
                }
                if group.is_required_set() {
                    constraints.push(format!("one required from: {}", members.join(", ")));
                }
            }
            output.push_str(&format!(
                "| `{name}` | `{}` | `{}` | {} | {} |\n",
                value.replace('|', "\\|"),
                if defaults.is_empty() { "-" } else { &defaults }.replace('|', "\\|"),
                if constraints.is_empty() {
                    "-".to_string()
                } else {
                    constraints.join("; ").replace('|', "\\|")
                },
                help.replace('|', "\\|")
            ));
        }
        output.push('\n');
    }
    output.push_str(&format!(
        "**Example**\n\n```sh\n{}\n```\n\n",
        command_example(path)
    ));
}

fn markdown_command_tree(command: &clap::Command, path: &str, output: &mut String) {
    markdown_command_body(command, path, output);
    for subcommand in command
        .get_subcommands()
        .filter(|subcommand| !subcommand.is_hide_set())
    {
        markdown_command_tree(
            subcommand,
            &format!("{path} {}", subcommand.get_name()),
            output,
        );
    }
}

fn reference_header() -> String {
    let exit_codes = crate::error::EXIT_CODE_DESCRIPTIONS
        .iter()
        .map(|(code, description)| format!("- `{code}`: {description}."))
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        "# Git Slop CLI Reference\n\nGenerated from the live Clap command tree.\n\n## Exit codes\n\n{exit_codes}\n\n"
    )
}

fn reference_markdown() -> String {
    let command = Cli::command();
    let mut markdown = reference_header();
    markdown_command_tree(&command, "git-slop", &mut markdown);
    format!("{}\n", markdown.trim_end())
}
