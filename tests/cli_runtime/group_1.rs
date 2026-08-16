#[test]
fn version_subcommand_preserves_public_shape() {
    let outside_repository = TempDir::new().expect("temporary non-repository directory");
    cargo_bin_cmd!("git-slop")
        .current_dir(outside_repository.path())
        .arg("version")
        .assert()
        .success()
        .stdout(format!("git-slop {}\n", env!("CARGO_PKG_VERSION")));
}

#[test]
fn build_info_reports_version_and_source_identity_as_json() {
    let outside_repository = TempDir::new().expect("temporary non-repository directory");
    let output = cargo_bin_cmd!("git-slop")
        .current_dir(outside_repository.path())
        .args(["build-info", "--format", "json"])
        .output()
        .expect("run build-info");
    assert!(output.status.success());
    let payload: Value = serde_json::from_slice(&output.stdout).expect("parse build-info JSON");
    assert_eq!(payload["schema_version"], 2);
    assert_eq!(payload["project"], "git-slop");
    assert_eq!(payload["version"], env!("CARGO_PKG_VERSION"));
    assert!(payload.get("source_revision").is_some());
    assert!(payload.get("source_dirty").is_some());
    assert!(
        payload["target"]
            .as_str()
            .is_some_and(|value| !value.is_empty())
    );
    assert!(payload.get("crate_sha256").is_some());
    assert!(
        payload["rustc_version"]
            .as_str()
            .is_some_and(|value| value.starts_with("rustc "))
    );
    assert!(matches!(
        payload["build_source"].as_str(),
        Some("workspace" | "crate" | "release")
    ));
}

#[test]
fn find_writes_schema_five_and_all_human_and_machine_surfaces() {
    let repository = committed_repository();
    cargo_bin_cmd!("git-slop")
        .current_dir(repository.path())
        .args(["find", "--persist-unadopted"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Repository Health"))
        .stdout(predicate::str::contains("Wrote report to"));

    let latest = repository.path().join(".slop/latest");
    for name in ["report.json", "summary.md", "health.md"] {
        assert!(latest.join(name).is_file(), "missing {name}");
    }
    assert!(!latest.join("report.yaml").exists());
    let report: Value = serde_json::from_slice(
        &fs::read(latest.join("report.json")).expect("read generated report"),
    )
    .expect("parse generated report");
    assert_eq!(report["schema_version"], 5);
    assert_eq!(report["files"].as_array().map(Vec::len), Some(3));
    assert!(report["health"]["findings"].is_array());
    assert!(report["repo"]["head_sha"].as_str().is_some());
    assert_eq!(
        report["overlays"]["organization_health"]["analysis_status"],
        "experimental"
    );
    assert_eq!(
        report["overlays"]["organization_health"]["analysis_version"],
        2
    );
    assert_eq!(
        report["overlays"]["organization_health"]["relationships"]["analysis_version"],
        2
    );
    assert_eq!(
        report["overlays"]["organization_health"]["clusters"]["analysis_version"],
        2
    );
    for key in [
        "duplicate_neighborhoods",
        "near_duplicate_neighborhoods",
        "temporal_coupling_edges",
        "lexical_affinity_edges",
        "boundary_leakage_edges",
    ] {
        assert!(report["overlays"]["organization_health"]["relationships"][key].is_array());
    }
    for key in [
        "duplicate_sets",
        "scattered_concepts",
        "boundary_leakage_clusters",
        "consolidation_candidates",
    ] {
        assert!(report["overlays"]["organization_health"]["clusters"][key].is_array());
    }
    for overlay in [
        "organization_health",
        "verification",
        "navigation",
        "blast_radius",
        "stewardship",
        "concept_dispersion",
    ] {
        assert_eq!(
            report["overlays"][overlay]["analysis_status"],
            "experimental"
        );
        assert_eq!(report["overlays"][overlay]["analysis_version"], 2);
    }
    assert!(report["overlays"]["concept_dispersion"]["findings"].is_array());

    let files = report["files"].as_array().expect("file records");
    let total_tokens: u64 = files
        .iter()
        .map(|file| file["tokens"].as_u64().expect("tokens"))
        .sum();
    let line_weights: Vec<u64> = files
        .iter()
        .map(|file| file["line_churn_window"].as_u64().expect("line churn"))
        .collect();
    let total_line_weight: u64 = line_weights.iter().sum();
    let entropy: f64 = line_weights
        .iter()
        .filter(|weight| **weight > 0)
        .map(|weight| {
            let probability = *weight as f64 / total_line_weight as f64;
            -probability * probability.log2()
        })
        .sum();
    let total_hunks: u64 = line_weights
        .iter()
        .map(|line_delta| (*line_delta).max(1).div_ceil(20))
        .sum();
    let raw_expected_diffusion = 0.35 * (3_f64.ln_1p() / 25_f64.ln()).min(1.0)
        + 0.25 * ((total_hunks as f64).ln_1p() / 50_f64.ln()).min(1.0)
        + 0.20 * (2_f64.ln_1p() / 10_f64.ln()).min(1.0)
        + 0.20 * (entropy / 3.0).min(1.0);
    let evidence_support = 1.0 / 6.0;
    let change_set_calibration = 1.0 / 2_f64.sqrt();
    let expected_diffusion = raw_expected_diffusion * change_set_calibration * evidence_support;
    let expected_diffusion_rounded = round6(expected_diffusion);

    for file in files {
        let tokens = file["tokens"].as_u64().expect("tokens");
        let path = file["path"].as_str().expect("path");
        let folder_token_count: u64 = files
            .iter()
            .filter(|candidate| {
                let candidate_path = candidate["path"].as_str().expect("candidate path");
                candidate_path.rsplit_once('/').map(|pair| pair.0)
                    == path.rsplit_once('/').map(|pair| pair.0)
            })
            .map(|candidate| candidate["tokens"].as_u64().expect("candidate tokens"))
            .sum();
        let load = &file["costs"]["load"];
        assert_eq!(load["file_token_count"], tokens);
        assert_eq!(load["folder_token_count"], folder_token_count);
        assert_close(
            load["top_file_share"].as_f64().expect("top file share"),
            round6(tokens as f64 / folder_token_count as f64),
        );
        assert_eq!(load["top_3_file_share"], 1.0);
        assert_close(
            load["token_concentration_ratio"]
                .as_f64()
                .expect("token concentration"),
            round6(tokens as f64 / total_tokens as f64),
        );

        let coordination = &file["costs"]["coordination"];
        let expected_cross_folder_ratio =
            (if path == "README.md" { 1.0 } else { 0.5 }) * evidence_support;
        assert_eq!(coordination["files_touched_per_change"], 3.0);
        assert_eq!(coordination["folders_touched_per_change"], 2.0);
        assert_eq!(coordination["edit_hunks_per_change"], 1.0);
        assert_eq!(coordination["cochange_degree"], 2);
        assert_eq!(coordination["cochange_centrality"], 1.0);
        assert_close(
            coordination["cross_folder_cochange_ratio"]
                .as_f64()
                .expect("cross-folder ratio"),
            round6(expected_cross_folder_ratio),
        );
        assert_eq!(coordination["cochange_pagerank"], 0.333333);
        assert_close(
            coordination["change_diffusion"]
                .as_f64()
                .expect("change diffusion"),
            expected_diffusion_rounded,
        );
        assert_close(
            coordination["coordination_pressure"]
                .as_f64()
                .expect("coordination pressure"),
            round6(
                (0.5 * expected_diffusion_rounded
                    + 0.3
                    + 0.2 * round6(expected_cross_folder_ratio))
                .min(1.0),
            ),
        );
    }
    assert!(
        fs::read_to_string(latest.join("health.md"))
            .expect("health report")
            .contains("# Repository Health")
    );
}
