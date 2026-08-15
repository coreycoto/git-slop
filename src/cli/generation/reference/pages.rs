fn reference_page_header(title: &str) -> String {
    format!(
        "# Git Slop CLI Reference: `{title}`\n\nGenerated from the live Clap command tree.\n\n"
    )
}

fn reference_bundle_page(
    command: &clap::Command,
    name: &str,
    path: &str,
) -> (String, std::collections::BTreeMap<String, String>) {
    let mut page = reference_page_header(name);
    let mut nested_pages = std::collections::BTreeMap::new();
    if name != "list" {
        markdown_command_tree(command, path, &mut page);
        return (format!("{}\n", page.trim_end()), nested_pages);
    }

    markdown_command_body(command, path, &mut page);
    page.push_str("## Subcommands\n\n");
    for subcommand in command.get_subcommands() {
        let subcommand_name = subcommand.get_name();
        let subcommand_path = format!("{path} {subcommand_name}");
        let filename = format!("{name}-{subcommand_name}.md");
        page.push_str(&format!("- [{subcommand_path}]({filename})\n"));

        let mut nested_page = reference_page_header(&format!("{name} {subcommand_name}"));
        markdown_command_tree(subcommand, &subcommand_path, &mut nested_page);
        nested_pages.insert(filename, format!("{}\n", nested_page.trim_end()));
    }
    (format!("{}\n", page.trim_end()), nested_pages)
}
