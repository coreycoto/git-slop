#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct BenchmarkProvenance {
    harness_revision: String,
    binary_sha256: String,
    binary_source_revision: String,
    binary_version: String,
    binary_target: String,
    binary_build_source: String,
    binary_inference_feature_enabled: bool,
}

fn collect_benchmark_provenance(
    options: &Options,
    binary: &Path,
) -> Result<BenchmarkProvenance> {
    let harness_revision = git_output(&options.repo_root, &["rev-parse", "HEAD"])?;
    let harness_status = git_output(
        &options.repo_root,
        &["status", "--porcelain=v1", "--untracked-files=all"],
    )?;
    if !harness_status.is_empty() {
        bail!(
            "benchmark_harness_dirty: commit or remove every harness worktree change before collecting benchmark evidence"
        );
    }
    let binary_sha256 = sha256_file(binary, 512 * 1024 * 1024, "benchmark binary")?;
    let build_info = command_output_bounded(
        Command::new(binary).args(["build-info", "--format", "json"]),
        64 * 1024,
        "benchmark binary build-info",
    )?;
    if !build_info.status.success() {
        bail!("benchmark binary did not provide build-info provenance");
    }
    let build_info: Value = serde_json::from_slice(&build_info.stdout)?;
    let source_revision = build_info["source_revision"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("benchmark binary has no source revision"))?;
    if source_revision != harness_revision || build_info["source_dirty"] != false {
        bail!(
            "benchmark_binary_unbound: benchmark binary must be a clean build of harness revision {harness_revision}"
        );
    }
    let doctor = command_output_bounded(
        Command::new(binary)
            .args(["--repo"])
            .arg(&options.repo_root)
            .args(["doctor", "--format", "json"]),
        2 * 1024 * 1024,
        "benchmark binary doctor receipt",
    )?;
    if !doctor.status.success() {
        bail!("benchmark binary did not provide its capability receipt");
    }
    let doctor: Value = serde_json::from_slice(&doctor.stdout)?;
    let inference_feature_enabled = doctor
        .pointer("/advisor/benchmark_feature_compiled")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    if !options.prepare_only && !inference_feature_enabled {
        bail!(
            "benchmark_binary_feature_missing: inference runs require a candidate built with advisor-inference-benchmark"
        );
    }
    Ok(BenchmarkProvenance {
        harness_revision,
        binary_sha256,
        binary_source_revision: source_revision.to_string(),
        binary_version: build_info["version"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("benchmark binary has no version"))?
            .to_string(),
        binary_target: build_info["target"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("benchmark binary has no target"))?
            .to_string(),
        binary_build_source: build_info["build_source"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("benchmark binary has no build source"))?
            .to_string(),
        binary_inference_feature_enabled: inference_feature_enabled,
    })
}

#[cfg(test)]
fn test_provenance() -> BenchmarkProvenance {
    BenchmarkProvenance {
        harness_revision: "1".repeat(40),
        binary_sha256: "2".repeat(64),
        binary_source_revision: "1".repeat(40),
        binary_version: env!("CARGO_PKG_VERSION").to_string(),
        binary_target: "test-target".to_string(),
        binary_build_source: "workspace".to_string(),
        binary_inference_feature_enabled: true,
    }
}
