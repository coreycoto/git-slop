fn selector_value(selector: &AdviceSelector) -> Value {
    match selector {
        AdviceSelector::Path(path) => json!({"kind": "path", "value": path}),
        AdviceSelector::Relationship(id) => json!({"kind": "relationship", "value": id}),
        AdviceSelector::Cluster(id) => json!({"kind": "cluster", "value": id}),
        AdviceSelector::Top(count) => json!({"kind": "top", "value": count}),
    }
}

fn refresh_excerpt(excerpt: &mut Value, maximum: usize) {
    let Some(text) = excerpt.get("text").and_then(Value::as_str) else {
        return;
    };
    let returned = truncate_utf8(text, maximum).to_string();
    let kind = excerpt
        .get("kind")
        .and_then(Value::as_str)
        .unwrap_or("source");
    let path = excerpt
        .get("path")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let content_digest = excerpt
        .get("content_sha256")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let excerpt_id = format!(
        "excerpt-{}",
        &sha256(format!(
            "{kind}\0{path}\0{content_digest}\0{}",
            returned.len()
        ))[..16]
    );
    excerpt["id"] = json!(excerpt_id);
    excerpt["line_range"]["end"] = json!(returned.lines().count().max(1));
    excerpt["excerpt_sha256"] = json!(sha256(returned.as_bytes()));
    excerpt["returned_bytes"] = json!(returned.len());
    excerpt["truncated"] = json!(true);
    excerpt["text"] = json!(returned);
}

fn refresh_excerpt_index(input: &mut Value) {
    let excerpt_ids = input["repository_excerpts"]
        .as_array()
        .expect("repository excerpts are an array")
        .iter()
        .filter_map(|item| item.get("id").and_then(Value::as_str))
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>();
    input["reference_index"]["excerpts"] = json!(excerpt_ids);
}

fn push_missing_evidence(input: &mut Value, path: String, reason: &'static str) {
    let missing = input["missing_evidence"]
        .as_array_mut()
        .expect("missing evidence is an array");
    if missing.iter().any(|item| {
        item.get("path").and_then(Value::as_str) == Some(path.as_str())
            && item.get("reason").and_then(Value::as_str) == Some(reason)
    }) {
        return;
    }
    missing.push(json!({"path": path, "reason": reason}));
}

fn compact_single_candidate_details(input: &mut Value) -> bool {
    let Some(candidates) = input["candidates"].as_array_mut() else {
        return false;
    };
    if candidates.len() != 1 {
        return false;
    }
    let candidate = &mut candidates[0];
    let Some(facts) = candidate
        .get_mut("observed_facts")
        .and_then(Value::as_object_mut)
    else {
        return false;
    };
    for key in ["scope_paths", "out_of_scope_paths"] {
        if let Some(paths) = facts.get_mut(key).and_then(Value::as_array_mut) {
            paths.truncate(1);
        }
    }
    if let Some(evidence) = facts.get_mut("evidence").and_then(Value::as_object_mut) {
        for key in ["finding_ids", "relationship_ids", "cluster_ids"] {
            if let Some(values) = evidence.get_mut(key).and_then(Value::as_array_mut) {
                values.truncate(1);
            }
        }
    }
    if let Some(expected) = facts
        .get_mut("expected_outcome")
        .and_then(Value::as_object_mut)
    {
        if let Some(required) = expected.get_mut("required").and_then(Value::as_array_mut) {
            required.truncate(1);
        }
    }
    if let Some(verification) = facts
        .get_mut("verification")
        .and_then(Value::as_object_mut)
    {
        verification.remove("concrete_targets");
        verification.remove("discovered_commands");
        verification.remove("required_checks");
        verification.insert(
            "references".to_string(),
            json!("reference_index"),
        );
    }
    if let Some(interpretation) = candidate
        .get_mut("interpretation")
        .and_then(Value::as_object_mut)
    {
        interpretation.remove("assumptions");
    }
    push_missing_evidence(
        input,
        "candidate-context".to_string(),
        "context_token_budget",
    );
    input["limits"]["truncated"] = json!(true);
    true
}

fn fit_token_budget(input: &mut Value, maximum: usize) -> Result<()> {
    let encoder = o200k_harmony().context("unable to initialize the o200k_harmony tokenizer")?;
    const TOKEN_ACCOUNTING_RESERVE: usize = 16;
    let mut candidate_details_compacted = false;
    for _ in 0..256 {
        let mut provisional = input.clone();
        provisional["limits"]["estimated_context_tokens"] = json!(0);
        provisional["context_digest"] = json!("0".repeat(64));
        let tokens = encoder
            .encode_ordinary(&serde_json::to_string(&provisional)?)
            .len();
        if tokens.saturating_add(TOKEN_ACCOUNTING_RESERVE) <= maximum {
            return Ok(());
        }
        let excess = tokens.saturating_sub(maximum);
        let excerpts = input["repository_excerpts"]
            .as_array_mut()
            .expect("repository excerpts are an array");
        if let Some(excerpt) = excerpts.iter_mut().rev().find(|excerpt| {
            excerpt
                .get("returned_bytes")
                .and_then(Value::as_u64)
                .unwrap_or_default()
                > 512
        }) {
            let previous = excerpt["returned_bytes"].as_u64().unwrap_or_default() as usize;
            let reduction = excess.saturating_mul(4).max(256);
            let target = previous.saturating_sub(reduction).max(512);
            refresh_excerpt(excerpt, target);
            let current = excerpt["returned_bytes"].as_u64().unwrap_or_default() as usize;
            let remaining = input["limits"]["remaining_bytes"]
                .as_u64()
                .unwrap_or_default() as usize;
            input["limits"]["remaining_bytes"] =
                json!(remaining.saturating_add(previous.saturating_sub(current)));
            input["limits"]["truncated"] = json!(true);
            refresh_excerpt_index(input);
            continue;
        }
        let Some(removed) = excerpts.pop() else {
            if !candidate_details_compacted && compact_single_candidate_details(input) {
                candidate_details_compacted = true;
                continue;
            }
            bail!(
                "candidates and policies require approximately {} tokens, exceeding the configured {maximum}-token advice context budget",
                tokens.saturating_add(TOKEN_ACCOUNTING_RESERVE)
            );
        };
        let path = removed
            .get("path")
            .and_then(Value::as_str)
            .unwrap_or("unknown")
            .to_string();
        push_missing_evidence(input, path, "context_token_budget");
        refresh_excerpt_index(input);
        input["limits"]["truncated"] = json!(true);
    }
    bail!("unable to fit deterministic advice input within its token budget")
}
