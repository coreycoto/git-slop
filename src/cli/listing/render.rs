fn merge_missing_fields(target: &mut Value, source: &Value) {
    let (Some(target), Some(source)) = (target.as_object_mut(), source.as_object()) else {
        return;
    };
    for (key, value) in source {
        if !target.contains_key(key) || target.get(key).is_some_and(Value::is_null) {
            target.insert(key.clone(), value.clone());
        }
    }
}

fn deduplicate_clusters(values: Vec<Value>) -> Vec<Value> {
    let mut indexed = std::collections::BTreeMap::<String, Value>::new();
    let mut anonymous = Vec::new();
    for item in values {
        let Some(id) = item.get("id").and_then(Value::as_str).map(ToOwned::to_owned) else {
            anonymous.push(item);
            continue;
        };
        match indexed.entry(id) {
            std::collections::btree_map::Entry::Vacant(entry) => {
                entry.insert(item);
            }
            std::collections::btree_map::Entry::Occupied(mut entry) => {
                let existing_is_candidate = entry
                    .get()
                    .get("kind")
                    .and_then(Value::as_str)
                    == Some("consolidation_candidate");
                let incoming_is_candidate = item.get("kind").and_then(Value::as_str)
                    == Some("consolidation_candidate");
                if existing_is_candidate && !incoming_is_candidate {
                    let mut preferred = item;
                    merge_missing_fields(&mut preferred, entry.get());
                    entry.insert(preferred);
                } else {
                    merge_missing_fields(entry.get_mut(), &item);
                }
            }
        }
    }
    indexed.into_values().chain(anonymous).collect()
}

fn evidence_score(item: &Value) -> f64 {
    item.get("evidence_score")
        .or_else(|| item.get("evidence_lower_bound"))
        .and_then(Value::as_f64)
        .unwrap_or_default()
}

fn rank_list_values(kind: &str, values: &mut [Value]) {
    match kind {
        "relationships" | "clusters" => values.sort_by(|left, right| {
            evidence_score(right)
                .partial_cmp(&evidence_score(left))
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| {
                    left.get("id")
                        .and_then(Value::as_str)
                        .cmp(&right.get("id").and_then(Value::as_str))
                })
        }),
        "profiles" => values.sort_by(|left, right| {
            left.get("name")
                .and_then(Value::as_str)
                .cmp(&right.get("name").and_then(Value::as_str))
        }),
        _ => {}
    }
}

fn render_findings_table(values: &[Value], output: &ListOutputArgs) {
    let terminal_width = std::env::var("COLUMNS")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(100);
    let path_width = if output.no_truncate {
        values
            .iter()
            .filter_map(|item| item.get("path").and_then(Value::as_str))
            .map(str::len)
            .max()
            .unwrap_or(24)
            .max(24)
    } else if output.wide {
        80
    } else {
        terminal_width.saturating_sub(59).clamp(24, 48)
    };
    println!(
        "{:<path_width$}  {:<16}  {:<10}  {:<10}  {:<10}  {:>7}",
        "PATH", "PROFILE", "SEVERITY", "CONTEXT", "SLOP", "SCORE"
    );
    println!(
        "{:-<path_width$}  {:-<16}  {:-<10}  {:-<10}  {:-<10}  {:-<7}",
        "", "", "", "", "", ""
    );
    for item in values {
        let label = item.get("path").and_then(Value::as_str).unwrap_or("-");
        let profile = item.get("profile").and_then(Value::as_str).unwrap_or("-");
        let severity = item.get("severity").and_then(Value::as_str).unwrap_or("-");
        let context = item
            .get("context_band")
            .and_then(Value::as_str)
            .unwrap_or("-");
        let slop = item.get("slop_band").and_then(Value::as_str).unwrap_or("-");
        let score = item
            .get("slop_score")
            .and_then(Value::as_f64)
            .map_or_else(|| "-".to_string(), |value| format!("{value:.3}"));
        println!(
            "{:<path_width$}  {:<16}  {:<10}  {:<10}  {:<10}  {:>7}",
            terminal_field(label, path_width, output.no_truncate),
            terminal_field(profile, 16, output.no_truncate),
            terminal_field(severity, 10, output.no_truncate),
            terminal_field(context, 10, output.no_truncate),
            terminal_field(slop, 10, output.no_truncate),
            score
        );
    }
}

fn render_relationships_table(values: &[Value], output: &ListOutputArgs) {
    println!(
        "{:<34}  {:<24}  {:>9}  {:>7}  {:>10}",
        "RELATIONSHIP", "KIND", "EVIDENCE", "SUPPORT", "CONFIDENCE"
    );
    println!("{:-<34}  {:-<24}  {:-<9}  {:-<7}  {:-<10}", "", "", "", "", "");
    let endpoint_width = if output.no_truncate || output.wide { 72 } else { 46 };
    for item in values {
        let id = item.get("id").and_then(Value::as_str).unwrap_or("-");
        let kind = item.get("kind").and_then(Value::as_str).unwrap_or("-");
        let support = item.get("support_count").and_then(Value::as_u64).unwrap_or_default();
        let confidence = item
            .get("confidence")
            .and_then(Value::as_str)
            .unwrap_or("-");
        println!(
            "{:<34}  {:<24}  {:>9.3}  {:>7}  {:>10}",
            terminal_field(id, 34, output.no_truncate),
            terminal_field(kind, 24, output.no_truncate),
            evidence_score(item),
            support,
            terminal_field(confidence, 10, output.no_truncate)
        );
        let source = item.get("source_path").and_then(Value::as_str).unwrap_or("-");
        let target = item.get("target_path").and_then(Value::as_str).unwrap_or("-");
        println!(
            "  {} → {}",
            terminal_field(source, endpoint_width, output.no_truncate),
            terminal_field(target, endpoint_width, output.no_truncate)
        );
    }
}

fn render_clusters_table(values: &[Value], output: &ListOutputArgs) {
    println!(
        "{:<34}  {:<26}  {:>7}  {:>9}  {:<24}",
        "CLUSTER", "KIND", "MEMBERS", "EVIDENCE", "CANDIDATE"
    );
    println!("{:-<34}  {:-<26}  {:-<7}  {:-<9}  {:-<24}", "", "", "", "", "");
    let member_width = if output.no_truncate || output.wide { 100 } else { 86 };
    for item in values {
        let id = item.get("id").and_then(Value::as_str).unwrap_or("-");
        let kind = item.get("kind").and_then(Value::as_str).unwrap_or("-");
        let members = item
            .get("member_count")
            .and_then(Value::as_u64)
            .or_else(|| item.get("member_paths").and_then(Value::as_array).map(|v| v.len() as u64))
            .unwrap_or_default();
        let candidate = item
            .get("candidate_type")
            .and_then(Value::as_str)
            .unwrap_or("-");
        println!(
            "{:<34}  {:<26}  {:>7}  {:>9.3}  {:<24}",
            terminal_field(id, 34, output.no_truncate),
            terminal_field(kind, 26, output.no_truncate),
            members,
            evidence_score(item),
            terminal_field(candidate, 24, output.no_truncate)
        );
        let member_paths = item
            .get("member_paths")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .collect::<Vec<_>>()
            .join(", ");
        println!("  {}", terminal_field(&member_paths, member_width, output.no_truncate));
    }
}

fn render_profiles_table(values: &[Value]) {
    println!(
        "{:<20}  {:>7}  {:>12}  {:>10}  {:>10}  {:>10}  {:>10}",
        "PROFILE", "FILES", "TOKENS", "LINES", "CODE", "COMMENTS", "BLANKS"
    );
    println!("{:-<20}  {:-<7}  {:-<12}  {:-<10}  {:-<10}  {:-<10}  {:-<10}", "", "", "", "", "", "", "");
    for item in values {
        let totals = item.get("totals").unwrap_or(&Value::Null);
        let total = |key: &str| totals.get(key).and_then(Value::as_u64).unwrap_or_default();
        println!(
            "{:<20}  {:>7}  {:>12}  {:>10}  {:>10}  {:>10}  {:>10}",
            terminal_field(item.get("name").and_then(Value::as_str).unwrap_or("-"), 20, false),
            total("files"),
            total("tokens"),
            total("lines"),
            total("code"),
            total("comments"),
            total("blanks")
        );
    }
}
