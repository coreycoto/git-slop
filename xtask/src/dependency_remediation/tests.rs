use super::candidate;
use super::{path_allowed, read_result, validate_result_paths};
use serde_json::json;
use std::{fs, path::PathBuf};
use tempfile::TempDir;

#[test]
fn caller_path_policy_accepts_only_declared_source_and_dependency_manifests() {
    for path in [
        "src/example.rs",
        "tests/unit.test.ts",
        "action/action.yml",
        "tools/generate.ts",
        "Cargo.toml",
        "Cargo.lock",
        "requirements-dev.txt",
        "tools/cli/Cargo.toml",
        "tools/cli/package.json",
        "tests/fixtures/package-lock.json",
        "src/line\nbreak.rs",
    ] {
        assert!(
            path_allowed(path),
            "allowed dependency input was rejected: {path:?}"
        );
    }

    for path in [
        "../outside.rs",
        "src/../outside.rs",
        "scripts/publish.sh",
        "/absolute.rs",
        "src\\outside.rs",
        "src//empty-component.rs",
        "src/trailing/",
        "docs/fixture/Cargo.toml",
        "src/name\0suffix.rs",
        "",
    ] {
        assert!(
            !path_allowed(path),
            "unsafe or out-of-scope path was accepted: {path:?}"
        );
    }
}

#[test]
fn proposal_result_requires_verified_shape_and_valid_source_inventory() {
    let temp = TempDir::new().unwrap();
    let path: PathBuf = temp.path().join("result.json");
    let result = json!({
        "status":"patched",
        "summary":"Update dependencies",
        "title":"chore: update dependencies",
        "body":"Apply tested dependency updates.",
        "supplemental_patch":"diff --git a/src/lib.rs b/src/lib.rs\n",
        "draft":true,
        "verification":["cargo test --locked"],
        "changed_files":["Cargo.toml","Cargo.lock"]
    });
    fs::write(&path, serde_json::to_vec(&result).unwrap()).unwrap();
    assert_eq!(read_result(&path).unwrap(), result);

    let mut invalid = result.clone();
    invalid["status"] = json!("failed");
    fs::write(&path, serde_json::to_vec(&invalid).unwrap()).unwrap();
    assert!(read_result(&path).is_err());

    let mut invalid = result.clone();
    invalid["changed_files"] = json!(["scripts/publish.sh"]);
    fs::write(&path, serde_json::to_vec(&invalid).unwrap()).unwrap();
    assert!(read_result(&path).is_err());
}

#[test]
fn result_file_inventory_rejects_duplicates_and_bad_types() {
    let valid = json!({
        "changed_files": ["Cargo.toml", "src/lib.rs", "src/line\nbreak.rs"]
    });
    assert_eq!(
        validate_result_paths(&valid).unwrap(),
        ["Cargo.toml", "src/lib.rs", "src/line\nbreak.rs"]
    );

    for invalid in [
        json!({"changed_files": []}),
        json!({"changed_files": ["src/lib.rs", "src/lib.rs"]}),
        json!({"changed_files": ["src/../outside.rs"]}),
        json!({"changed_files": [42]}),
        json!({"changed_files": ["src/file\0name.rs"]}),
        json!({"changed_files": null}),
    ] {
        assert!(
            validate_result_paths(&invalid).is_err(),
            "invalid result was accepted: {invalid}"
        );
    }
}

#[test]
fn candidate_application_binds_staged_paths_and_rejects_test_tree_changes() {
    use std::process::Command;

    let temp = TempDir::new().unwrap();
    let root = temp.path().join("source");
    let proposal = temp.path().join("proposal");
    let tree_file = temp.path().join("candidate-tree.sha");
    fs::create_dir_all(root.join("src")).unwrap();
    fs::create_dir_all(proposal.join("publication")).unwrap();
    let git = |args: &[&str]| {
        let output = Command::new("git")
            .args(args)
            .current_dir(&root)
            .output()
            .unwrap();
        assert!(output.status.success(), "git {args:?} failed");
        output.stdout
    };
    git(&["init", "-q"]);
    git(&["config", "user.name", "Fixture"]);
    git(&["config", "user.email", "fixture@example.invalid"]);
    fs::write(root.join("src/lib.rs"), "before\n").unwrap();
    git(&["add", "src/lib.rs"]);
    git(&["commit", "-qm", "base"]);
    let source_sha = String::from_utf8(git(&["rev-parse", "HEAD"]))
        .unwrap()
        .trim()
        .to_owned();

    fs::write(root.join("src/lib.rs"), "after\n").unwrap();
    let patch = git(&["diff", "--binary", "HEAD"]);
    fs::write(proposal.join("publication/patch.diff"), &patch).unwrap();
    let result = json!({
        "status":"patched","summary":"Update dependency","title":"chore: update dependency",
        "body":"Apply verified update.","supplemental_patch":String::from_utf8(patch).unwrap(),
        "draft":true,"verification":["cargo test --locked"],"changed_files":["src/lib.rs"]
    });
    fs::write(
        proposal.join("publication/result.json"),
        serde_json::to_vec(&result).unwrap(),
    )
    .unwrap();
    git(&["checkout", "--", "src/lib.rs"]);

    candidate::apply(&root, &proposal, &source_sha, &tree_file).unwrap();
    assert_eq!(
        fs::read_to_string(root.join("src/lib.rs")).unwrap(),
        "after\n",
        "qualification must materialize and test the supplemental patch in the worktree"
    );
    let qualified = Command::new("sh")
        .args(["-c", "test \"$(cat src/lib.rs)\" = after"])
        .current_dir(&root)
        .status()
        .unwrap();
    assert!(
        qualified.success(),
        "credential-free candidate qualification must observe the patched source"
    );
    let expected_tree = fs::read_to_string(&tree_file).unwrap();
    assert_eq!(
        String::from_utf8(git(&["write-tree"])).unwrap().trim(),
        expected_tree
    );
    candidate::verify(&root, &source_sha, &tree_file).unwrap();

    fs::write(root.join("src/lib.rs"), "test changed tracked source\n").unwrap();
    assert!(candidate::verify(&root, &source_sha, &tree_file).is_err());

    fs::write(root.join("src/lib.rs"), "after\n").unwrap();
    candidate::verify(&root, &source_sha, &tree_file).unwrap();

    fs::write(root.join("test-output.txt"), "created by the test\n").unwrap();
    assert!(
        candidate::verify(&root, &source_sha, &tree_file).is_err(),
        "credential-free tests must not leave untracked source files"
    );
    fs::remove_file(root.join("test-output.txt")).unwrap();
    candidate::verify(&root, &source_sha, &tree_file).unwrap();

    fs::write(root.join("src/lib.rs"), "test changed staged source\n").unwrap();
    git(&["add", "src/lib.rs"]);
    assert!(
        candidate::verify(&root, &source_sha, &tree_file).is_err(),
        "credential-free tests must not alter the exact staged candidate tree"
    );
    fs::write(root.join("src/lib.rs"), "after\n").unwrap();
    git(&["add", "src/lib.rs"]);
    candidate::verify(&root, &source_sha, &tree_file).unwrap();
    git(&["commit", "--allow-empty", "-qm", "test changed HEAD"]);
    assert!(
        candidate::verify(&root, &source_sha, &tree_file).is_err(),
        "candidate tests must not replace the captured source commit"
    );
}
