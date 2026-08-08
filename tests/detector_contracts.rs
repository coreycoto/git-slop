use std::fs;
use std::process::Command;

use git_slop::run_find_in;
use serde_json::Value;
use tempfile::TempDir;

fn git(repository: &TempDir, args: &[&str]) {
    let output = Command::new("git")
        .current_dir(repository.path())
        .args(args)
        .output()
        .expect("run git");
    assert!(
        output.status.success(),
        "git {} failed: {}",
        args.join(" "),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn commit_all(repository: &TempDir, message: &str) {
    git(repository, &["add", "--all"]);
    git(
        repository,
        &["-c", "commit.gpgsign=false", "commit", "-m", message],
    );
}

fn duplicate_source(revision: usize) -> String {
    let block = [
        "pub fn shared_logic(value: &str) -> Vec<String> {",
        "    let normalized = value.trim().to_ascii_lowercase();",
        "    normalized.split_whitespace().map(str::to_owned).collect()",
        "}",
    ]
    .join("\n");
    format!(
        "{}\n// paired revision {revision}\n",
        vec![block; 30].join("\n")
    )
}

fn write_duplicate_pair(repository: &TempDir, left: &str, right: &str, revision: usize) {
    let source = duplicate_source(revision);
    fs::write(repository.path().join(left), &source).expect("write left duplicate");
    fs::write(repository.path().join(right), source).expect("write right duplicate");
}

fn relationship_for_pair<'a>(
    report: &'a Value,
    kind: &str,
    left: &str,
    right: &str,
) -> Option<&'a Value> {
    report["relationships"][kind]
        .as_array()?
        .iter()
        .find(|relationship| {
            relationship["source_path"] == left && relationship["target_path"] == right
        })
}

fn cluster_contains_pair(cluster: &Value, left: &str, right: &str) -> bool {
    cluster["member_paths"].as_array().is_some_and(|paths| {
        paths.contains(&Value::String(left.to_string()))
            && paths.contains(&Value::String(right.to_string()))
    })
}

fn record_for_path<'a>(records: &'a Value, path: &str) -> &'a Value {
    records
        .as_array()
        .expect("record array")
        .iter()
        .find(|record| record["path"] == path)
        .unwrap_or_else(|| panic!("missing record for {path}"))
}

fn initialize_repository() -> TempDir {
    let repository = TempDir::new().expect("temporary repository");
    git(&repository, &["init", "-b", "main"]);
    git(&repository, &["config", "user.name", "Git Slop Tests"]);
    git(
        &repository,
        &["config", "user.email", "git-slop-tests@example.invalid"],
    );
    fs::write(repository.path().join(".gitignore"), ".slop/\n").expect("gitignore");
    for directory in ["src", "pkg", "docs"] {
        fs::create_dir_all(repository.path().join(directory)).expect("fixture directory");
    }
    repository
}

#[test]
fn native_detector_emits_duplicate_coupling_clusters_and_refreshes_latest() {
    let repository = initialize_repository();
    let original_left = "src/alpha.rs";
    let original_right = "pkg/beta.rs";
    let obsolete = "docs/obsolete.md";

    write_duplicate_pair(&repository, original_left, original_right, 0);
    fs::write(
        repository.path().join(obsolete),
        "# Obsolete\n\nThis tracked file should disappear from the refreshed report.\n",
    )
    .expect("obsolete fixture");
    commit_all(&repository, "initial structure");

    for revision in 1..=3 {
        write_duplicate_pair(&repository, original_left, original_right, revision);
        commit_all(&repository, &format!("paired change {revision}"));
    }

    let first = run_find_in(repository.path()).expect("run native detector");
    assert!(
        first.report["diagnostics"]["analysis"]["cache_misses"]
            .as_u64()
            .unwrap_or_default()
            > 0
    );
    let report = &first.report;
    let duplicate = relationship_for_pair(
        report,
        "duplicate_neighborhoods",
        original_right,
        original_left,
    )
    .expect("exact duplicate relationship");
    assert_eq!(duplicate["kind"], "duplicate_neighborhood");
    assert_eq!(duplicate["similarity"], 1.0);
    assert!(
        relationship_for_pair(
            report,
            "near_duplicate_neighborhoods",
            original_right,
            original_left,
        )
        .is_none()
    );

    let coupling = relationship_for_pair(
        report,
        "temporal_coupling_edges",
        original_right,
        original_left,
    )
    .expect("temporal coupling relationship");
    assert!(coupling["support_count"].as_u64().unwrap_or_default() >= 3);
    assert!(coupling["evidence_score"].as_f64().unwrap_or_default() > 0.0);

    for cluster_kind in ["duplicate_sets", "consolidation_candidates"] {
        assert!(
            report["clusters"][cluster_kind]
                .as_array()
                .expect("cluster array")
                .iter()
                .any(|cluster| cluster_contains_pair(cluster, original_right, original_left)),
            "missing {cluster_kind} for the duplicate pair"
        );
    }

    let left_file = record_for_path(&report["files"], original_left);
    let right_file = record_for_path(&report["files"], original_right);
    assert!(left_file.get("structural_tokens").is_none());
    assert!(right_file.get("structural_tokens").is_none());
    assert!(left_file.get("content_fingerprint").is_none());
    assert!(right_file.get("content_fingerprint").is_none());

    let organization = record_for_path(&report["organization_metrics"]["files"], original_left);
    assert!(
        organization["duplication_pressure"]
            .as_f64()
            .unwrap_or_default()
            > 0.0
    );
    assert!(
        organization["coupling_pressure"]
            .as_f64()
            .unwrap_or_default()
            > 0.0
    );
    assert!(
        organization["boundary_pressure"]
            .as_f64()
            .unwrap_or_default()
            > 0.0
    );
    assert!(
        organization["cross_boundary_edge_count"]
            .as_u64()
            .unwrap_or_default()
            > 0
    );
    assert!(
        organization["relationship_ids"]
            .as_array()
            .is_some_and(|ids| !ids.is_empty())
    );
    assert!(
        organization["cluster_ids"]
            .as_array()
            .is_some_and(|ids| !ids.is_empty())
    );
    let blast_radius = record_for_path(&report["overlays"]["blast_radius"]["files"], original_left);
    assert!(
        blast_radius["blast_radius_pressure"]
            .as_f64()
            .unwrap_or_default()
            > 0.0
    );

    let first_latest = fs::read_to_string(repository.path().join(".slop/latest/report.json"))
        .expect("first latest report");
    assert!(first_latest.contains(original_left));
    assert!(first_latest.contains(obsolete));
    fs::write(
        repository.path().join(".slop/latest/stale-only.txt"),
        "must be removed by the next atomic latest refresh\n",
    )
    .expect("stale latest sentinel");

    let renamed_left = "engine/renamed_alpha.rs";
    fs::create_dir_all(repository.path().join("engine")).expect("renamed fixture directory");
    git(&repository, &["mv", original_left, renamed_left]);
    git(&repository, &["rm", obsolete]);
    commit_all(&repository, "rename duplicate and remove obsolete file");

    let second = run_find_in(repository.path()).expect("rerun native detector");
    assert!(
        second.report["diagnostics"]["analysis"]["cache_hits"]
            .as_u64()
            .unwrap_or_default()
            > 0,
        "unchanged files should reuse the content-addressed token cache"
    );
    assert!(record_for_path(&second.report["files"], renamed_left).is_object());
    assert!(
        relationship_for_pair(
            &second.report,
            "duplicate_neighborhoods",
            renamed_left,
            original_right,
        )
        .is_some()
    );
    for artifact in ["report.json", "report.yaml", "summary.md", "health.md"] {
        let contents = fs::read_to_string(repository.path().join(".slop/latest").join(artifact))
            .unwrap_or_else(|error| panic!("read refreshed {artifact}: {error}"));
        assert!(
            !contents.contains(original_left),
            "stale rename in {artifact}"
        );
        assert!(!contents.contains(obsolete), "stale deletion in {artifact}");
    }
    assert!(
        !repository
            .path()
            .join(".slop/latest/stale-only.txt")
            .exists(),
        "atomic latest refresh retained an obsolete artifact"
    );
}
