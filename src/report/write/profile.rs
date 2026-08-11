fn cap_collection(report: &mut Value, key: &str, limit: usize) {
    let Some(records) = report.get_mut(key).and_then(Value::as_array_mut) else {
        return;
    };
    let total = records.len();
    records.truncate(limit);
    report["collection_metadata"][key] = json!({
        "total": total,
        "returned": records.len(),
        "limit": limit,
        "truncated": total > records.len()
    });
}

fn collect_prioritized_paths(report: &Value) -> Vec<String> {
    fn push_path(path: Option<&str>, seen: &mut BTreeSet<String>, paths: &mut Vec<String>) {
        if let Some(path) = path.filter(|path| !path.is_empty()) {
            if seen.insert(path.to_string()) {
                paths.push(path.to_string());
            }
        }
    }

    let mut seen = BTreeSet::new();
    let mut paths = Vec::new();
    for pointer in [
        "/health/findings",
        "/health/refactor_candidates",
        "/health/watchlist",
        "/action_queue",
        "/observation_feed",
        "/ranked_files",
    ] {
        for record in report
            .pointer(pointer)
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            push_path(
                record.get("path").and_then(Value::as_str),
                &mut seen,
                &mut paths,
            );
        }
    }
    if let Some(relationships) = report
        .pointer("/overlays/organization_health/relationships")
        .and_then(Value::as_object)
    {
        for records in relationships.values().filter_map(Value::as_array) {
            for record in records {
                push_path(
                    record.get("source_path").and_then(Value::as_str),
                    &mut seen,
                    &mut paths,
                );
                push_path(
                    record.get("target_path").and_then(Value::as_str),
                    &mut seen,
                    &mut paths,
                );
            }
        }
    }
    paths
}

fn compact_files(report: &mut Value, limit: usize) -> BTreeSet<String> {
    let priorities = collect_prioritized_paths(report);
    let records = report
        .get_mut("files")
        .and_then(Value::as_array_mut)
        .expect("canonical reports contain files");
    let total = records.len();
    let original = std::mem::take(records);
    let mut by_path = original
        .iter()
        .filter_map(|record| Some((record.get("path")?.as_str()?.to_string(), record.clone())))
        .collect::<BTreeMap<_, _>>();
    for path in priorities {
        if records.len() >= limit {
            break;
        }
        if let Some(record) = by_path.remove(&path) {
            records.push(record);
        }
    }
    for record in original {
        if records.len() >= limit {
            break;
        }
        let Some(path) = record.get("path").and_then(Value::as_str) else {
            continue;
        };
        if by_path.remove(path).is_some() {
            records.push(record);
        }
    }
    let retained = records
        .iter()
        .filter_map(|record| record.get("path").and_then(Value::as_str))
        .map(ToOwned::to_owned)
        .collect::<BTreeSet<_>>();
    report["collection_metadata"]["files"] = json!({
        "total": total,
        "returned": records.len(),
        "limit": limit,
        "truncated": total > records.len()
    });
    retained
}

fn retain_path_collection(
    report: &mut Value,
    pointer: &str,
    metadata_key: &str,
    retained: &BTreeSet<String>,
    limit: Option<usize>,
) {
    let Some(records) = report.pointer_mut(pointer).and_then(Value::as_array_mut) else {
        return;
    };
    let total = records.len();
    records.retain(|record| {
        record
            .get("path")
            .and_then(Value::as_str)
            .is_some_and(|path| retained.contains(path))
    });
    if let Some(limit) = limit {
        records.truncate(limit);
    }
    let returned = records.len();
    report["collection_metadata"][metadata_key] = json!({
        "total": total,
        "returned": returned,
        "limit": limit,
        "truncated": total > returned
    });
}

fn retain_relationship_references(report: &mut Value, retained: &BTreeSet<String>) {
    let Some(relationships) = report
        .pointer_mut("/overlays/organization_health/relationships")
        .and_then(Value::as_object_mut)
    else {
        return;
    };
    for records in relationships.values_mut().filter_map(Value::as_array_mut) {
        records.retain(|record| {
            ["source_path", "target_path"].into_iter().all(|key| {
                record
                    .get(key)
                    .and_then(Value::as_str)
                    .is_none_or(|path| retained.contains(path))
            })
        });
    }
}

fn retain_cluster_references(report: &mut Value, retained: &BTreeSet<String>) {
    let Some(clusters) = report
        .pointer_mut("/overlays/organization_health/clusters")
        .and_then(Value::as_object_mut)
    else {
        return;
    };
    for records in clusters.values_mut().filter_map(Value::as_array_mut) {
        records.retain(|record| {
            record
                .get("member_paths")
                .and_then(Value::as_array)
                .is_none_or(|members| {
                    members
                        .iter()
                        .filter_map(Value::as_str)
                        .all(|path| retained.contains(path))
                })
        });
    }
}

fn apply_report_profile(report: &mut Value, profile: &str) {
    report["diagnostics"]["report_profile"] = json!(profile);
    report["diagnostics"]["report_profile_semantics"] = json!(match profile {
        "compact" =>
            "bounded presentation with exhaustive comparison index and resolvable retained references",
        "standard" =>
            "complete primary records with bounded high-cardinality relationship evidence",
        "full_evidence" => "complete primary records and unbounded retained evidence",
        _ => "unknown report profile",
    });
    if profile == "full_evidence" {
        return;
    }
    if profile == "standard" {
        for pointer in [
            "/overlays/organization_health/relationships/duplicate_neighborhoods",
            "/overlays/organization_health/relationships/near_duplicate_neighborhoods",
            "/overlays/organization_health/relationships/temporal_coupling_edges",
            "/overlays/organization_health/relationships/lexical_affinity_edges",
            "/overlays/organization_health/relationships/boundary_leakage_edges",
        ] {
            if let Some(records) = report.pointer_mut(pointer).and_then(Value::as_array_mut) {
                records.truncate(2_000);
            }
        }
        return;
    }
    let retained = compact_files(report, 250);
    for (pointer, metadata_key, limit) in [
        ("/health/findings", "health.findings", None),
        (
            "/health/refactor_candidates",
            "health.refactor_candidates",
            None,
        ),
        ("/health/watchlist", "health.watchlist", None),
        ("/action_queue", "action_queue", Some(100)),
        ("/observation_feed", "observation_feed", Some(250)),
        ("/ranked_files", "ranked_files", Some(250)),
    ] {
        retain_path_collection(report, pointer, metadata_key, &retained, limit);
    }
    retain_relationship_references(report, &retained);
    retain_cluster_references(report, &retained);
    cap_collection(report, "folders", 250);
    report["diagnostics"]["compact_profile_note"] = json!(
        "Collections are deterministically bounded; use --report-profile full-evidence for complete records."
    );
}
