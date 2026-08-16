pub fn failing_records_in(
    report: &Value,
    fail_on_context_band: Option<&str>,
    fail_on_slop_band: Option<&str>,
    include_folders: bool,
) -> Vec<Value> {
    fn context_rank(value: &str) -> i32 {
        match value {
            "compact" => 0,
            "healthy" => 1,
            "warning" => 2,
            "critical" | "refactor_required" | "budget_exceeded" => 3,
            _ => -1,
        }
    }
    fn slop_rank(value: &str) -> i32 {
        match value {
            "low" => 0,
            "moderate" => 1,
            "high" => 2,
            "critical" => 3,
            _ => -1,
        }
    }
    let collections: &[&str] = if include_folders {
        &["files", "folders"]
    } else {
        &["files"]
    };
    let indexed = report.get("policy_index").and_then(Value::as_object);
    let mut failures: Vec<Value> = collections
        .iter()
        .flat_map(|collection| {
            let records: &[Value] = indexed
                .and_then(|index| index.get(*collection))
                .and_then(Value::as_array)
                .map(Vec::as_slice)
                .unwrap_or_else(|| array_at(report, &[*collection]));
            records.iter().map(move |record| (*collection, record))
        })
        .filter(|record| {
            let record = record.1;
            if matches!(
                string(record.get("classification")).as_str(),
                "generated" | "vendored" | "snapshot" | "fixture" | "migration_fixture"
            ) {
                return false;
            }
            let context_failed = fail_on_context_band
                .map(|threshold| {
                    context_rank(&string(record.get("context_band"))) >= context_rank(threshold)
                })
                .unwrap_or(false);
            let slop_failed = fail_on_slop_band
                .map(|threshold| {
                    slop_rank(&string(record.get("slop_band"))) >= slop_rank(threshold)
                })
                .unwrap_or(false);
            context_failed || slop_failed
        })
        .map(|(collection, record)| {
            let mut record = record.clone();
            record["record_type"] = json!(if collection == "files" {
                "file"
            } else {
                "folder"
            });
            record
        })
        .collect();
    failures.sort_by(|left, right| {
        cmp_f64_desc(
            number(left.get("slop_score")),
            number(right.get("slop_score")),
        )
        .then_with(|| usize_value(right.get("tokens")).cmp(&usize_value(left.get("tokens"))))
        .then_with(|| string(left.get("path")).cmp(&string(right.get("path"))))
    });
    failures
}
