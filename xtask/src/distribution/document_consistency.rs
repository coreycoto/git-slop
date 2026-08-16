fn validate_document_consistency(repo_root: &Path, errors: &mut Vec<String>) {
    let readme = match fs::read_to_string(repo_root.join("README.md")) {
        Ok(text) => text,
        Err(error) => {
            errors.push(format!("Unable to read README.md: {error}"));
            return;
        }
    };
    if !readme.contains("writes three required files plus optional YAML") {
        errors.push(
            "README.md report-bundle count must say three required files plus optional YAML."
                .into(),
        );
    }
    let report_rows = ["report.json", "report.yaml", "summary.md", "health.md"]
        .iter()
        .filter(|artifact| readme.contains(&format!("| `{artifact}` |")))
        .count();
    if report_rows != 4 || !readme.contains("`report.yaml` | Optional") {
        errors.push(
            "README.md report-bundle table must contain four rows and mark report.yaml optional."
                .into(),
        );
    }

    let checklist = match fs::read_to_string(repo_root.join("docs/release-checklist.md")) {
        Ok(text) => text,
        Err(error) => {
            errors.push(format!("Unable to read docs/release-checklist.md: {error}"));
            return;
        }
    };
    if checklist.contains("must all verify before\n")
        || !checklist.contains("must all verify before inspecting or publishing the draft.")
    {
        errors.push(
            "docs/release-checklist.md must complete the pre-publication verification sentence."
                .into(),
        );
    }
}
