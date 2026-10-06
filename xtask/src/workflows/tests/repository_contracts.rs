#[test]
fn every_external_action_surface_requires_a_full_commit_sha() {
    let root = tempfile::tempdir().unwrap();
    let workflows = root.path().join(".github/workflows");
    fs::create_dir_all(&workflows).unwrap();
    fs::write(
        root.path().join("action.yml"),
        "runs:\n  using: composite\n  steps:\n    - uses: actions/cache@0123456789abcdef0123456789abcdef01234567\n",
    )
    .unwrap();
    fs::write(
        workflows.join("unsafe.yml"),
        "jobs:\n  unsafe:\n    uses: owner/reusable@v1\n",
    )
    .unwrap();
    let mut errors = Vec::new();
    validate_action_versions(root.path(), &workflows, &mut errors);
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert!(errors[0].contains("owner/reusable@v1"));
}

#[test]
fn packaged_contract_validation_requires_a_clean_fixture() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let valid = fs::read_to_string(root.join("scripts/validate-packaged-contracts.sh")).unwrap();
    let invalid = valid.replacen(
        "git clone --quiet --no-hardlinks --no-tags \"$source_worktree\" \"$worktree\"",
        "cp -R \"$source_worktree\" \"$worktree\"",
        1,
    );
    let mut errors = Vec::new();
    validate_packaged_contracts_text(&invalid, &mut errors);
    assert!(errors.iter().any(|error| error.contains("git clone")));
}
