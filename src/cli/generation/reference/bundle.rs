fn reference_bundle() -> (String, std::collections::BTreeMap<String, String>) {
    let command = Cli::command();
    let mut index = reference_header();
    markdown_command_body(&command, "git-slop", &mut index);
    index.push_str("## Commands\n\n");
    let mut pages = std::collections::BTreeMap::new();
    for subcommand in command
        .get_subcommands()
        .filter(|subcommand| !subcommand.is_hide_set())
    {
        let name = subcommand.get_name();
        let path = format!("git-slop {name}");
        let filename = format!("{name}.md");
        index.push_str(&format!("- [{path}](cli-reference/{filename})\n"));
        let (page, nested_pages) = reference_bundle_page(subcommand, name, &path);
        pages.insert(filename, page);
        pages.extend(nested_pages);
    }
    (format!("{}\n", index.trim_end()), pages)
}

fn reference_page_dir(output: &Path) -> Result<PathBuf> {
    let stem = output
        .file_stem()
        .filter(|value| !value.is_empty())
        .context("reference output must have a file name")?;
    Ok(output
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join(stem))
}

fn run_reference(args: ReferenceArgs) -> Result<i32> {
    let Some(output) = args.output.as_deref() else {
        write_generated_output(None, reference_markdown().as_bytes())?;
        return Ok(0);
    };
    let (index, pages) = reference_bundle();
    write_generated_output(Some(output), index.as_bytes())?;
    let page_dir = reference_page_dir(output)?;
    fs::create_dir_all(&page_dir)?;
    for (filename, page) in pages {
        config::write_text_atomically(&page_dir.join(filename), page, false)?;
    }
    Ok(0)
}

#[cfg(test)]
mod generated_reference_tests {
    use std::collections::BTreeSet;

    #[test]
    fn committed_cli_reference_bundle_matches_the_live_command_tree() {
        let docs = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("docs");
        let (index, pages) = super::reference_bundle();
        assert_eq!(
            std::fs::read_to_string(docs.join("cli-reference.md"))
                .unwrap()
                .replace("\r\n", "\n"),
            index
        );
        let page_dir = docs.join("cli-reference");
        let actual = std::fs::read_dir(&page_dir)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .filter(|name| name.ends_with(".md"))
            .collect::<BTreeSet<_>>();
        assert_eq!(actual, pages.keys().cloned().collect());
        for (filename, expected) in pages {
            assert_eq!(
                std::fs::read_to_string(page_dir.join(filename))
                    .unwrap()
                    .replace("\r\n", "\n"),
                expected
            );
        }
    }
}
