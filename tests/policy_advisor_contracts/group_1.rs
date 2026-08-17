#[test]
fn policy_pack_lifecycle_is_offline_locked_and_inspectable() {
    let repository = repository();
    let cache = repository.path().join("isolated-policy-cache");
    let pack = repository.path().join("team-policy");
    let command = |arguments: &[&str]| {
        cargo_bin_cmd!("git-slop")
            .current_dir(repository.path())
            .env("GIT_SLOP_POLICY_HOME", &cache)
            .args(arguments)
            .output()
            .expect("run policy command")
    };

    assert!(
        command(&["policy", "init", "team-policy", "--format", "json"])
            .status
            .success()
    );
    assert!(
        command(&["policy", "validate", "team-policy", "--format", "json"])
            .status
            .success()
    );
    assert!(
        command(&["policy", "test", "team-policy", "--format", "json"])
            .status
            .success()
    );
    let install = command(&[
            "policy",
            "install",
            "team-policy",
            "--select",
            "--format",
            "json"
        ]);
    assert!(install.status.success());
    let install: Value = serde_json::from_slice(&install.stdout).expect("install JSON");
    assert_eq!(
        install["mutation"]["class"],
        "user_cache_install_and_repository_selection"
    );
    assert_eq!(
        install["mutation"]["repository_changed_paths"],
        json!([".slop/policies.yaml"])
    );
    assert_eq!(
        install["mutation"]["unselect_command"],
        "git slop policy remove com.example.repository-policy --unselect"
    );

    let lock_output = command(&["policy", "lock", "--format", "json"]);
    assert!(lock_output.status.success());
    let lock_output: Value = serde_json::from_slice(&lock_output.stdout).expect("lock JSON");
    assert_eq!(lock_output["mutation"]["class"], "repository_policy_lock");
    assert_eq!(
        lock_output["mutation"]["commit"]["paths"],
        json!([".slop/policies.yaml", ".slop/policy-lock.json"])
    );

    let listed = command(&["policy", "list", "--format", "json"]);
    assert!(listed.status.success());
    let listed: Value = serde_json::from_slice(&listed.stdout).expect("list JSON");
    assert!(listed["packs"].as_array().is_some_and(|packs| {
        packs
            .iter()
            .any(|pack| pack["id"] == "org.git-slop.core" && pack["built_in"] == true)
            && packs.iter().any(|pack| {
                pack["id"] == "com.example.repository-policy" && pack["selected"] == true
            })
    }));
    let lock = read_json(repository.path().join(".slop/policy-lock.json"));
    assert_eq!(lock["schema_version"], 1);
    assert_eq!(lock["packs"][0]["id"], "org.git-slop.core");
    assert_eq!(lock["resolution_digest"].as_str().map(str::len), Some(64));

    fs::write(
        pack.join("policies/repository.md"),
        "changed after installation\n",
    )
    .expect("change source only");
    assert!(
        command(&[
            "policy",
            "show",
            "com.example.repository-policy",
            "--format",
            "json"
        ])
        .status
        .success()
    );
    let index = read_json(cache.join("index.json"));
    let digest = index["packs"]["com.example.repository-policy"]
        .as_str()
        .expect("cached pack digest");
    fs::write(
        cache.join(digest).join("policies/repository.md"),
        "tampered installed policy\n",
    )
    .expect("tamper installed pack");
    let tampered = command(&[
        "policy",
        "show",
        "com.example.repository-policy",
        "--format",
        "json",
    ]);
    assert!(!tampered.status.success());
    assert!(String::from_utf8_lossy(&tampered.stderr).contains("no longer matches"));
    let remove = command(&[
            "policy",
            "remove",
            "com.example.repository-policy",
            "--unselect",
            "--format",
            "json"
        ]);
    assert!(remove.status.success());
    let remove: Value = serde_json::from_slice(&remove.stdout).expect("remove JSON");
    assert_eq!(
        remove["mutation"]["class"],
        "user_cache_removal_and_repository_unselection"
    );
    assert_eq!(remove["lock_invalidated"], true);
    assert_eq!(
        remove["mutation"]["repository_changed_paths"],
        json!([".slop/policies.yaml", ".slop/policy-lock.json"])
    );
    assert!(!repository.path().join(".slop/policy-lock.json").exists());
}
