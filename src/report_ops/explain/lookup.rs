use super::*;

pub(super) fn require_path_record(report: &Value, target_path: &str) -> Result<Value> {
    resolved_record(report, target_path).ok_or_else(|| {
        let indexed = ["files", "folders"].iter().any(|collection| {
            array_at(report, &["compare_index", collection])
                .iter()
                .any(|record| record.get("path").and_then(Value::as_str) == Some(target_path))
        });
        let path = visible_controls(target_path);
        if indexed {
            anyhow!(
                "Detailed evidence for '{path}' was omitted from this report. Run `git slop find --report-profile standard` with the same repository and scope, then retry explain using the new report."
            )
        } else {
            anyhow!("No record found for '{path}'.")
        }
    })
}

#[cfg(test)]
mod lookup_tests {
    use super::*;

    #[test]
    fn indexed_file_and_folder_omissions_are_explicit_and_escape_controls() {
        for collection in ["files", "folders"] {
            let mut report = json!({"files": [], "folders": [], "compare_index": {}});
            report["compare_index"][collection] = json!([{"path": "omitted\npath"}]);
            let message = require_path_record(&report, "omitted\npath")
                .expect_err("index lacks detailed explanation evidence")
                .to_string();
            assert!(message.contains("Detailed evidence for 'omitted\\npath' was omitted"));
            assert!(!message.contains('\n'));
            assert!(message.contains("--report-profile standard"));
        }
    }
}
