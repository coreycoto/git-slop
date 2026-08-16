fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

fn resolve(root: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        root.join(path)
    }
}

fn safe_slug(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 100
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

fn sha256(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    hex::encode(Sha256::digest(bytes))
}

fn semantic_report_sha256(bytes: &[u8]) -> Result<String> {
    let mut report: Value = serde_json::from_slice(bytes)?;
    for pointer in [
        "/diagnostics/analysis/analysis_elapsed_ms_before_report",
        "/diagnostics/analysis/estimator_error_ratio",
        "/diagnostics/analysis/measured_peak_rss_bytes",
        "/diagnostics/report_sizes/logical_artifact_bytes",
        "/diagnostics/report_sizes/report_json_bytes",
    ] {
        if let Some(value) = report.pointer_mut(pointer) {
            *value = Value::Null;
        }
    }
    Ok(sha256(&serde_json::to_vec(&report)?))
}

fn validate_corpus(corpus: &Corpus) -> Result<()> {
    if corpus.schema_version != 1
        || corpus.id.trim().is_empty()
        || corpus.license.trim().is_empty()
        || corpus.review_status != "maintainer-reviewed"
        || corpus.privacy.trim().is_empty()
        || corpus.repositories.is_empty()
        || corpus.cases.is_empty()
    {
        bail!("advisor corpus does not satisfy schema-1 identity requirements");
    }
    for (key, repository) in &corpus.repositories {
        if !safe_slug(key)
            || repository.revision.len() != 40
            || !repository
                .revision
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit())
            || chrono_like_rfc3339(&repository.as_of).is_none()
            || repository
                .expected_report_sha256
                .as_ref()
                .is_some_and(|digest| {
                    digest.len() != 64 || !digest.bytes().all(|byte| byte.is_ascii_hexdigit())
                })
        {
            bail!("invalid benchmark repository fixture {key:?}");
        }
    }
    let mut ids = BTreeSet::new();
    for case in &corpus.cases {
        if !ids.insert(&case.id)
            || !safe_slug(&case.id)
            || !corpus.repositories.contains_key(&case.repository)
            || case.selector.len() != 2
            || !["--path", "--relationship", "--cluster", "--top"]
                .contains(&case.selector[0].as_str())
            || !(1..=20).contains(&case.candidate_count)
            || ![
                "unmodified",
                "detector-rewrite",
                "test-weakening",
                "inventory-evasion",
                "unjustified-scope-expansion",
                "missing-evidence",
            ]
            .contains(&case.scenario.as_str())
            || case.scenario_tags.is_empty()
            || !["approve", "abstain", "revise", "reject"]
                .contains(&case.expected_aggregate.as_str())
            || case.expected_rule_verdicts.is_empty()
            || case.expected_rule_verdicts.values().any(|verdict| {
                !["approve", "abstain", "revise", "reject"].contains(&verdict.as_str())
            })
        {
            bail!("invalid or duplicate advisor corpus case {}", case.id);
        }
    }
    Ok(())
}

fn chrono_like_rfc3339(value: &str) -> Option<()> {
    let bytes = value.as_bytes();
    (bytes.len() == 20
        && bytes[4] == b'-'
        && bytes[7] == b'-'
        && bytes[10] == b'T'
        && bytes[13] == b':'
        && bytes[16] == b':'
        && bytes[19] == b'Z'
        && bytes.iter().enumerate().all(|(index, byte)| {
            matches!(index, 4 | 7 | 10 | 13 | 16 | 19) || byte.is_ascii_digit()
        }))
    .then_some(())
}

fn git_output(root: &Path, args: &[&str]) -> Result<String> {
    let output = Command::new("git").current_dir(root).args(args).output()?;
    if !output.status.success() {
        bail!("benchmark repository did not satisfy a required Git inspection");
    }
    Ok(String::from_utf8(output.stdout)?.trim().to_string())
}

fn repository_map(values: &[String], corpus: &Corpus) -> Result<BTreeMap<String, PathBuf>> {
    let mut repositories = BTreeMap::new();
    for value in values {
        let (key, path) = value
            .split_once('=')
            .ok_or_else(|| anyhow::anyhow!("--repository must use KEY=PATH: {value:?}"))?;
        if !safe_slug(key) || path.is_empty() || repositories.contains_key(key) {
            bail!("invalid or duplicate benchmark repository mapping {value:?}");
        }
        let path = fs::canonicalize(path)
            .with_context(|| format!("unable to resolve benchmark repository {key}"))?;
        let fixture = corpus.repositories.get(key).ok_or_else(|| {
            anyhow::anyhow!("repository mapping {key:?} is not declared by the corpus")
        })?;
        let root = fs::canonicalize(git_output(&path, &["rev-parse", "--show-toplevel"])?)?;
        if root != path {
            bail!("benchmark repository {key} must map to its Git worktree root");
        }
        if git_output(&path, &["rev-parse", "HEAD"])? != fixture.revision {
            bail!(
                "benchmark repository {key} is not at the corpus-pinned revision {}",
                fixture.revision
            );
        }
        if !git_output(
            &path,
            &["status", "--porcelain=v1", "--untracked-files=all"],
        )?
        .is_empty()
        {
            bail!("benchmark repository {key} must be a clean disposable checkout");
        }
        repositories.insert(key.to_string(), path);
    }
    let missing = corpus
        .repositories
        .keys()
        .filter(|key| !repositories.contains_key(*key))
        .cloned()
        .collect::<Vec<_>>();
    if !missing.is_empty() {
        bail!(
            "missing benchmark repository mappings: {}",
            missing.join(", ")
        );
    }
    Ok(repositories)
}

#[derive(Debug)]
struct PreparedReport {
    path: PathBuf,
    sha256: String,
}

fn prepare_reports(
    binary: &Path,
    repositories: &BTreeMap<String, PathBuf>,
    corpus: &Corpus,
    workspace: &Path,
) -> Result<BTreeMap<String, PreparedReport>> {
    let mut reports = BTreeMap::new();
    for (key, repository) in repositories {
        let fixture = corpus
            .repositories
            .get(key)
            .expect("mapped repository is declared by corpus");
        let root = workspace.join(key);
        let state = root.join("state");
        let output_root = root.join("reports");
        fs::create_dir_all(&root)?;
        let output = Command::new(binary)
            .current_dir(repository)
            .args(["--repo", repository.to_string_lossy().as_ref(), "find"])
            .arg("--state-dir")
            .arg(&state)
            .arg("--output-dir")
            .arg(&output_root)
            .args([
                "--no-cache",
                "--quiet",
                "--allow-shallow",
                "--as-of",
                &fixture.as_of,
            ])
            .output()?;
        if !output.status.success() {
            bail!("deterministic benchmark report generation failed for {key}");
        }
        let path = output_root.join("latest/report.json");
        let bytes = fs::read(&path)
            .with_context(|| format!("benchmark report generation produced no report for {key}"))?;
        let digest = semantic_report_sha256(&bytes)?;
        if fixture
            .expected_report_sha256
            .as_ref()
            .is_some_and(|expected| expected != &digest)
        {
            bail!("benchmark report for {key} drifted from the reviewed corpus fingerprint");
        }
        reports.insert(
            key.clone(),
            PreparedReport {
                path,
                sha256: digest,
            },
        );
    }
    Ok(reports)
}
