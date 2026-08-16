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
    assert!(
        command(&[
            "policy",
            "install",
            "team-policy",
            "--select",
            "--format",
            "json"
        ])
        .status
        .success()
    );
    assert!(
        command(&["policy", "lock", "--format", "json"])
            .status
            .success()
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
    assert!(
        command(&[
            "policy",
            "remove",
            "com.example.repository-policy",
            "--unselect",
            "--format",
            "json"
        ])
        .status
        .success()
    );
    assert!(!repository.path().join(".slop/policy-lock.json").exists());
}
