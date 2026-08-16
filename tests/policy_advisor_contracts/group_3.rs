#[test]
fn policy_pack_rejects_symlink_and_reserved_core_namespace() {
    let repository = repository();
    let cache = repository.path().join("isolated-policy-cache");
    cargo_bin_cmd!("git-slop")
        .current_dir(repository.path())
        .env("GIT_SLOP_POLICY_HOME", &cache)
        .args(["policy", "init", "unsafe-policy"])
        .assert()
        .success();
    let manifest = repository.path().join("unsafe-policy/git-slop-policy.yaml");
    let original = fs::read_to_string(&manifest).expect("manifest");
    let text = original
        .clone()
        .replace("com.example.repository-policy", "org.git-slop.core");
    fs::write(&manifest, text).expect("reserved manifest");
    cargo_bin_cmd!("git-slop")
        .current_dir(repository.path())
        .env("GIT_SLOP_POLICY_HOME", &cache)
        .args(["policy", "validate", "unsafe-policy"])
        .assert()
        .code(3)
        .stderr(predicate::str::contains("reserved core namespace"));

    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;
        fs::write(&manifest, original).expect("restore manifest");
        let target = repository
            .path()
            .join("unsafe-policy/policies/repository.md");
        let _ = fs::remove_file(&target);
        symlink(repository.path().join("README.md"), &target).expect("policy symlink");
        cargo_bin_cmd!("git-slop")
            .current_dir(repository.path())
            .env("GIT_SLOP_POLICY_HOME", &cache)
            .args(["policy", "validate", "unsafe-policy"])
            .assert()
            .code(3)
            .stderr(predicate::str::contains("must not contain symlinks"));
    }
}
