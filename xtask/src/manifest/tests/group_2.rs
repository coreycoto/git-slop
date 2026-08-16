    #[test]
    fn manifest_validation_rejects_unknown_fields_and_contract_drift() -> Result<()> {
        let (temp, dist) = fixture()?;
        let mut runner = FakeRunner {
            outputs: VecDeque::from(["a".repeat(40)]),
            ..FakeRunner::default()
        };
        let manifest = build_manifest_with_runner(
            temp.path(),
            &dist,
            &crate_source(),
            Some("v0.9.0"),
            &mut runner,
        )?;
        manifest.validate()?;

        let mut unknown = serde_json::to_value(&manifest)?;
        unknown
            .as_object_mut()
            .unwrap()
            .insert("unexpected".into(), serde_json::Value::Bool(true));
        assert!(serde_json::from_value::<ReleaseManifest>(unknown).is_err());

        let mut nested_unknown = serde_json::to_value(&manifest)?;
        nested_unknown["crate_source"]
            .as_object_mut()
            .unwrap()
            .insert("unexpected".into(), serde_json::Value::Bool(true));
        assert!(serde_json::from_value::<ReleaseManifest>(nested_unknown).is_err());

        let mut missing = manifest.clone();
        missing.artifacts.pop();
        assert!(
            missing
                .validate()
                .unwrap_err()
                .to_string()
                .contains("exactly 7")
        );

        let mut artifact_drift = manifest.clone();
        artifact_drift.artifacts[0].path = "renamed.tar.gz".into();
        assert!(
            artifact_drift
                .validate()
                .unwrap_err()
                .to_string()
                .contains("exact name and path")
        );

        let mut checksum_drift = manifest.clone();
        checksum_drift.checksums.name = "checksums.txt".into();
        assert!(
            checksum_drift
                .validate()
                .unwrap_err()
                .to_string()
                .contains("checksum metadata")
        );

        let mut install_drift = manifest;
        install_drift.install.homebrew_tap.pop();
        assert!(
            install_drift
                .validate()
                .unwrap_err()
                .to_string()
                .contains("install metadata")
        );
        Ok(())
    }

    #[test]
    fn release_artifact_size_bound_matches_the_public_action() -> Result<()> {
        assert_eq!(MAX_RELEASE_ARTIFACT_BYTES, 128 * 1024 * 1024);
        assert!(
            include_str!("../../../../action/install.mjs")
                .contains("const maximumArchiveBytes = 128 * 1024 * 1024;")
        );
        let (temp, dist) = fixture()?;
        let mut runner = FakeRunner {
            outputs: VecDeque::from(["a".repeat(40)]),
            ..FakeRunner::default()
        };
        let manifest = build_manifest_with_runner(
            temp.path(),
            &dist,
            &crate_source(),
            Some("v0.9.0"),
            &mut runner,
        )?;

        let mut at_limit = manifest.clone();
        at_limit.artifacts[0].size_bytes = MAX_RELEASE_ARTIFACT_BYTES;
        at_limit.validate()?;

        let mut over_limit = manifest;
        over_limit.artifacts[0].size_bytes = MAX_RELEASE_ARTIFACT_BYTES + 1;
        assert!(
            over_limit
                .validate()
                .unwrap_err()
                .to_string()
                .contains("size must be from 1 through")
        );

        let oversized_name = artifact_name("v0.9.0", RELEASE_TARGETS[0]);
        File::options()
            .write(true)
            .open(dist.join(oversized_name))?
            .set_len(MAX_RELEASE_ARTIFACT_BYTES + 1)?;
        let error = build_manifest_with_runner(
            temp.path(),
            &dist,
            &crate_source(),
            Some("v0.9.0"),
            &mut FakeRunner::default(),
        )
        .unwrap_err();
        assert!(error.to_string().contains("size must be from 1 through"));
        Ok(())
    }

    #[test]
    fn missing_and_unexpected_release_artifacts_fail_closed() -> Result<()> {
        let (temp, dist) = fixture()?;
        let missing = artifact_name("v0.9.0", RELEASE_TARGETS[1]);
        fs::remove_file(dist.join(&missing))?;
        let error = build_manifest_with_runner(
            temp.path(),
            &dist,
            &crate_source(),
            Some("v0.9.0"),
            &mut FakeRunner::default(),
        )
        .unwrap_err();
        assert!(
            error
                .to_string()
                .contains("missing required release artifact")
        );

        fs::write(dist.join(&missing), b"restored\n")?;
        fs::write(
            dist.join("git-slop-v0.9.0-riscv64gc-unknown-linux-gnu.tar.gz"),
            b"unsupported\n",
        )?;
        let error = build_manifest_with_runner(
            temp.path(),
            &dist,
            &crate_source(),
            Some("v0.9.0"),
            &mut FakeRunner::default(),
        )
        .unwrap_err();
        assert!(error.to_string().contains("unexpected release artifact"));
        Ok(())
    }
