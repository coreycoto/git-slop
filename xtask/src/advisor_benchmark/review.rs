const REVIEW_PROTOCOL: &str = "blind-independent-multi-repeat-v1";
const MINIMUM_REVIEWERS: usize = 2;
const MINIMUM_REPETITIONS_PER_CASE: usize = 2;

pub fn validate_operation_receipt(value: &Value) -> Result<()> {
    let schema: Value =
        serde_json::from_str(include_str!("../../../schemas/advisor-operation-receipt-1.json"))?;
    let validator = jsonschema::draft202012::options()
        .build(&schema)
        .context("embedded advisor operation receipt schema is invalid")?;
    if let Some(error) = validator.iter_errors(value).next() {
        bail!(
            "advisor operation receipt does not match schema 1 at {}: {}",
            error.instance_path(),
            error
        );
    }
    Ok(())
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ReviewManifest {
    schema_version: u64,
    status: String,
    protocol: String,
    source_results_sha256: String,
    blind_index_sha256: String,
    minimum_reviewers: usize,
    minimum_repetitions_per_case: usize,
    entries: Vec<ReviewManifestEntry>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ReviewManifestEntry {
    review_id: String,
    artifact_file: String,
    review_artifact_sha256: String,
    source_artifact_sha256: String,
    sample_sha256: String,
    case_id: String,
    reasoning_effort: String,
    context_token_limit: usize,
    repetition: usize,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RatingsFile {
    schema_version: u64,
    protocol: String,
    source_results_sha256: String,
    review_manifest_sha256: String,
    reviewers: Vec<ReviewerRatings>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ReviewerRatings {
    reviewer_id: String,
    independent: bool,
    blinded: bool,
    ratings: BTreeMap<String, CaseRating>,
}

fn sample_evidence_sha256(sample: &Sample) -> Result<String> {
    let mut value = serde_json::to_value(sample)?;
    value
        .as_object_mut()
        .expect("sample serializes as an object")
        .remove("sample_sha256");
    Ok(sha256(&serde_json::to_vec(&value)?))
}

fn seal_sample(mut sample: Sample) -> Result<Sample> {
    sample.sample_sha256 = sample_evidence_sha256(&sample)?;
    Ok(sample)
}

fn verify_sample_digest(sample: &Sample) -> Result<()> {
    if sample.sample_sha256 != sample_evidence_sha256(sample)? {
        bail!(
            "benchmark sample {} has a stale sample_sha256",
            sample.case_id
        );
    }
    Ok(())
}

fn blind_review_artifact(review_id: &str, sample: &Sample, artifact: &Value) -> Value {
    json!({
        "schema_version": 1,
        "review_id": review_id,
        "protocol": REVIEW_PROTOCOL,
        "source_artifact_sha256": sample.artifact_sha256,
        "sample_sha256": sample.sample_sha256,
        "blinding": {
            "provider_identity_removed": true,
            "runtime_identity_removed": true,
            "reasoning_effort_removed": true,
            "repetition_removed": true
        },
        "advice": {
            "candidate_ids": artifact.get("candidate_ids").cloned().unwrap_or(Value::Null),
            "context": artifact.get("context").cloned().unwrap_or(Value::Null),
            "policies": artifact.get("policies").cloned().unwrap_or(Value::Null),
            "evaluation": artifact.get("evaluation").cloned().unwrap_or(Value::Null),
            "validation": artifact.get("validation").cloned().unwrap_or(Value::Null),
            "boundary": artifact.get("boundary").cloned().unwrap_or(Value::Null)
        }
    })
}

fn validate_review_artifact(value: &Value) -> Result<()> {
    let schema: Value =
        serde_json::from_str(include_str!("../../../schemas/advisor-review-artifact-1.json"))?;
    let validator = jsonschema::draft202012::options()
        .build(&schema)
        .context("embedded advisor review artifact schema is invalid")?;
    if let Some(error) = validator.iter_errors(value).next() {
        bail!(
            "advisor review artifact does not match schema 1 at {}: {}",
            error.instance_path(),
            error
        );
    }
    Ok(())
}

fn record_review_artifact(
    directory: &Path,
    entries: &mut Vec<ReviewManifestEntry>,
    sample: &Sample,
    artifact: &Value,
) -> Result<()> {
    let source_artifact_sha256 = sample
        .artifact_sha256
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("valid review sample has no source artifact digest"))?;
    let review_id = format!("review-{:06}", entries.len().saturating_add(1));
    let artifact_file = format!("{review_id}.json");
    let review = blind_review_artifact(&review_id, sample, artifact);
    validate_review_artifact(&review)?;
    let bytes = serde_json::to_string_pretty(&review)? + "\n";
    write_review_artifact(directory, &artifact_file, bytes.as_bytes())?;
    entries.push(ReviewManifestEntry {
        review_id,
        artifact_file,
        review_artifact_sha256: sha256(bytes.as_bytes()),
        source_artifact_sha256: source_artifact_sha256.to_string(),
        sample_sha256: sample.sample_sha256.clone(),
        case_id: sample.case_id.clone(),
        reasoning_effort: sample.reasoning_effort.clone(),
        context_token_limit: sample.context_token_limit,
        repetition: sample.repetition,
    });
    Ok(())
}

fn validate_review_manifest(value: &Value) -> Result<()> {
    let schema: Value =
        serde_json::from_str(include_str!("../../../schemas/advisor-review-manifest-1.json"))?;
    let validator = jsonschema::draft202012::options()
        .build(&schema)
        .context("embedded advisor review manifest schema is invalid")?;
    if let Some(error) = validator.iter_errors(value).next() {
        bail!(
            "advisor review manifest does not match schema 1 at {}: {}",
            error.instance_path(),
            error
        );
    }
    Ok(())
}

fn write_review_manifests(
    directory: &Path,
    entries: &[ReviewManifestEntry],
    results_path: &Path,
    complete: bool,
) -> Result<()> {
    let source_results_sha256 = sha256_file(
        results_path,
        MAX_BENCHMARK_RESULT_BYTES as u64,
        "advisor benchmark source results",
    )?;
    let blind_index = json!({
        "schema_version": 1,
        "protocol": REVIEW_PROTOCOL,
        "source_results_sha256": source_results_sha256,
        "reviews": entries.iter().map(|entry| json!({
            "review_id": entry.review_id,
            "artifact_file": entry.artifact_file,
            "review_artifact_sha256": entry.review_artifact_sha256
        })).collect::<Vec<_>>()
    });
    let blind_index_bytes = serde_json::to_string_pretty(&blind_index)? + "\n";
    write_review_artifact(
        directory,
        "blind-review-index.json",
        blind_index_bytes.as_bytes(),
    )?;
    let manifest = ReviewManifest {
        schema_version: 1,
        status: if complete { "complete" } else { "incomplete" }.to_string(),
        protocol: REVIEW_PROTOCOL.to_string(),
        source_results_sha256,
        blind_index_sha256: sha256(blind_index_bytes.as_bytes()),
        minimum_reviewers: MINIMUM_REVIEWERS,
        minimum_repetitions_per_case: MINIMUM_REPETITIONS_PER_CASE,
        entries: entries.to_vec(),
    };
    let value = serde_json::to_value(&manifest)?;
    validate_review_manifest(&value)?;
    let bytes = serde_json::to_string_pretty(&value)? + "\n";
    write_review_artifact(directory, "review-manifest.json", bytes.as_bytes())?;
    Ok(())
}

fn safe_review_member(name: &str) -> bool {
    let path = Path::new(name);
    !name.is_empty()
        && path.components().count() == 1
        && matches!(path.components().next(), Some(std::path::Component::Normal(_)))
}

fn load_review_manifest(
    path: &Path,
    source_results_sha256: &str,
) -> Result<(ReviewManifest, String)> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        bail!("review manifest must be a regular file");
    }
    let bytes = read_bounded(path, MAX_BENCHMARK_CONFIG_BYTES, "advisor review manifest")?;
    let value: Value = serde_json::from_slice(&bytes)?;
    validate_review_manifest(&value)?;
    let manifest: ReviewManifest = serde_json::from_value(value)?;
    if manifest.status != "complete"
        || manifest.protocol != REVIEW_PROTOCOL
        || manifest.source_results_sha256 != source_results_sha256
        || manifest.minimum_reviewers != MINIMUM_REVIEWERS
        || manifest.minimum_repetitions_per_case != MINIMUM_REPETITIONS_PER_CASE
    {
        bail!("review manifest does not bind the completed source result and review protocol");
    }
    let parent = path
        .parent()
        .ok_or_else(|| anyhow::anyhow!("review manifest has no parent directory"))?;
    let blind_index = parent.join("blind-review-index.json");
    let blind_index_bytes = read_bounded(
        &blind_index,
        MAX_BENCHMARK_CONFIG_BYTES,
        "blind review index",
    )?;
    if sha256(&blind_index_bytes) != manifest.blind_index_sha256 {
        bail!("blind review index digest does not match the review manifest");
    }
    let expected_blind_index = json!({
        "schema_version": 1,
        "protocol": REVIEW_PROTOCOL,
        "source_results_sha256": source_results_sha256,
        "reviews": manifest.entries.iter().map(|entry| json!({
            "review_id": entry.review_id,
            "artifact_file": entry.artifact_file,
            "review_artifact_sha256": entry.review_artifact_sha256
        })).collect::<Vec<_>>()
    });
    if serde_json::from_slice::<Value>(&blind_index_bytes)? != expected_blind_index {
        bail!("blind review index does not match the private review manifest");
    }
    let mut review_ids = BTreeSet::new();
    let mut sample_ids = BTreeSet::new();
    for entry in &manifest.entries {
        if !review_ids.insert(entry.review_id.as_str())
            || !sample_ids.insert(entry.sample_sha256.as_str())
            || !safe_review_member(&entry.artifact_file)
        {
            bail!("review manifest contains duplicate or unsafe review evidence");
        }
        let artifact_path = parent.join(&entry.artifact_file);
        let artifact_metadata = fs::symlink_metadata(&artifact_path)?;
        if artifact_metadata.file_type().is_symlink() || !artifact_metadata.is_file() {
            bail!("review artifact must be a regular file: {}", entry.artifact_file);
        }
        let artifact_bytes = read_bounded(
            &artifact_path,
            MAX_BENCHMARK_CHILD_ARTIFACT_BYTES,
            "blind review artifact",
        )?;
        if sha256(&artifact_bytes) != entry.review_artifact_sha256 {
            bail!("review artifact digest drifted for {}", entry.review_id);
        }
        let artifact: Value = serde_json::from_slice(&artifact_bytes)?;
        validate_review_artifact(&artifact)?;
        if artifact["review_id"] != entry.review_id
            || artifact["source_artifact_sha256"] != entry.source_artifact_sha256
            || artifact["sample_sha256"] != entry.sample_sha256
        {
            bail!("review artifact identity drifted for {}", entry.review_id);
        }
    }
    Ok((manifest, sha256(&bytes)))
}

fn selected_review_ids(
    result: &Value,
    samples: &[Sample],
    manifest: &ReviewManifest,
) -> Result<BTreeSet<String>> {
    let effort = result
        .pointer("/recommended_configuration/reasoning_effort")
        .and_then(Value::as_str)
        .ok_or_else(|| {
            anyhow::anyhow!(
                "completed benchmark has no automatically eligible configuration to review"
            )
        })?;
    let selected_samples = samples
        .iter()
        .filter(|sample| {
            sample.status == "valid"
                && sample.reasoning_effort == effort
                && sample.context_token_limit == 8_192
        })
        .map(|sample| (sample.sample_sha256.as_str(), sample))
        .collect::<BTreeMap<_, _>>();
    let selected_entries = manifest
        .entries
        .iter()
        .filter(|entry| {
            entry.reasoning_effort == effort && entry.context_token_limit == 8_192
        })
        .collect::<Vec<_>>();
    if selected_entries.len() != selected_samples.len() {
        bail!("review manifest does not cover every valid selected benchmark sample");
    }
    let mut by_case = BTreeMap::<&str, BTreeSet<usize>>::new();
    let mut review_ids = BTreeSet::new();
    for entry in selected_entries {
        let sample = selected_samples
            .get(entry.sample_sha256.as_str())
            .ok_or_else(|| anyhow::anyhow!("review manifest references an unknown sample"))?;
        if entry.case_id != sample.case_id
            || entry.repetition != sample.repetition
            || entry.source_artifact_sha256
                != sample.artifact_sha256.as_deref().unwrap_or_default()
        {
            bail!("review manifest sample evidence drifted for {}", entry.review_id);
        }
        by_case
            .entry(&entry.case_id)
            .or_default()
            .insert(entry.repetition);
        review_ids.insert(entry.review_id.clone());
    }
    let expected_cases = samples
        .iter()
        .map(|sample| sample.case_id.as_str())
        .collect::<BTreeSet<_>>();
    if by_case.keys().copied().collect::<BTreeSet<_>>() != expected_cases
        || by_case
            .values()
            .any(|repetitions| repetitions.len() < MINIMUM_REPETITIONS_PER_CASE)
    {
        bail!(
            "blind review evidence must cover at least {MINIMUM_REPETITIONS_PER_CASE} repetitions of every corpus case"
        );
    }
    Ok(review_ids)
}

fn validate_ratings(value: &Value) -> Result<()> {
    let schema: Value =
        serde_json::from_str(include_str!("../../../schemas/advisor-ratings-2.json"))?;
    let validator = jsonschema::draft202012::options()
        .build(&schema)
        .context("embedded advisor ratings schema 2 is invalid")?;
    if let Some(error) = validator.iter_errors(value).next() {
        bail!(
            "advisor ratings do not match schema 2 at {}: {}",
            error.instance_path(),
            error
        );
    }
    Ok(())
}

fn reviewer_scores(reviewer: &ReviewerRatings) -> ReviewerScores {
    let count = reviewer.ratings.len() as f64;
    let mean = |select: fn(&CaseRating) -> f64| {
        reviewer.ratings.values().map(select).sum::<f64>() / count
    };
    let usefulness = mean(|rating| rating.recommendation_usefulness);
    let separation = mean(|rating| rating.fact_interpretation_separation);
    let scope = mean(|rating| rating.scope_quality);
    let verification = mean(|rating| rating.verification_quality);
    let actionability = mean(|rating| rating.actionability);
    ReviewerScores {
        reviewer_id: reviewer.reviewer_id.clone(),
        reviewed_artifact_count: reviewer.ratings.len(),
        recommendation_usefulness_mean: usefulness,
        fact_interpretation_separation_mean: separation,
        scope_quality_mean: scope,
        verification_quality_mean: verification,
        actionability_mean: actionability,
        overall_quality_mean: (usefulness + separation + scope + verification + actionability)
            / 5.0,
        unsupported_claim_count: reviewer
            .ratings
            .values()
            .map(|rating| rating.unsupported_claim_count)
            .sum(),
    }
}

fn ratings(
    path: &Path,
    source_results_sha256: &str,
    review_manifest_sha256: &str,
    selected_review_ids: &BTreeSet<String>,
) -> Result<ManualScores> {
    let bytes = read_bounded(path, MAX_BENCHMARK_CONFIG_BYTES, "advisor ratings")?;
    let value: Value = serde_json::from_slice(&bytes)?;
    validate_ratings(&value)?;
    let ratings: RatingsFile = serde_json::from_value(value)?;
    if ratings.schema_version != 2
        || ratings.protocol != REVIEW_PROTOCOL
        || ratings.source_results_sha256 != source_results_sha256
        || ratings.review_manifest_sha256 != review_manifest_sha256
        || ratings.reviewers.len() < MINIMUM_REVIEWERS
    {
        bail!("maintainer ratings do not bind the required blind review evidence");
    }
    let mut reviewer_ids = BTreeSet::new();
    for reviewer in &ratings.reviewers {
        if !reviewer_ids.insert(reviewer.reviewer_id.as_str())
            || !reviewer.independent
            || !reviewer.blinded
            || reviewer.ratings.keys().cloned().collect::<BTreeSet<_>>()
                != *selected_review_ids
        {
            bail!(
                "every independent blinded reviewer must rate every selected review artifact exactly"
            );
        }
    }
    let reviewer_scores = ratings
        .reviewers
        .iter()
        .map(reviewer_scores)
        .collect::<Vec<_>>();
    let reviewer_count = reviewer_scores.len();
    let reviewed_artifact_count = selected_review_ids.len();
    let mean = |select: fn(&ReviewerScores) -> f64| {
        reviewer_scores.iter().map(select).sum::<f64>() / reviewer_count as f64
    };
    let usefulness = mean(|score| score.recommendation_usefulness_mean);
    let separation = mean(|score| score.fact_interpretation_separation_mean);
    let scope = mean(|score| score.scope_quality_mean);
    let verification = mean(|score| score.verification_quality_mean);
    let actionability = mean(|score| score.actionability_mean);
    Ok(ManualScores {
        reviewer_count,
        reviewed_artifact_count,
        recommendation_usefulness_mean: usefulness,
        fact_interpretation_separation_mean: separation,
        scope_quality_mean: scope,
        verification_quality_mean: verification,
        actionability_mean: actionability,
        overall_quality_mean: (usefulness + separation + scope + verification + actionability)
            / 5.0,
        unsupported_claim_count: reviewer_scores
            .iter()
            .map(|score| score.unsupported_claim_count)
            .sum(),
        reviewer_scores,
    })
}
