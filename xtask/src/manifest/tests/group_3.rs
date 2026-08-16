    #[test]
    fn version_and_tag_must_agree_before_git_resolution() -> Result<()> {
        let (temp, dist) = fixture()?;
        let mut runner = FakeRunner::default();
        let error = build_manifest_with_runner(
            temp.path(),
            &dist,
            &crate_source(),
            Some("v0.9.1"),
            &mut runner,
        )
        .unwrap_err();
        assert_eq!(
            error.to_string(),
            "Cargo.toml version is 0.9.0; release tag v0.9.1 is 0.9.1."
        );
        assert!(runner.output_calls.is_empty());
        Ok(())
    }

    #[test]
    fn output_files_preserve_final_newlines() -> Result<()> {
        let (temp, dist) = fixture()?;
        let mut runner = FakeRunner {
            outputs: VecDeque::from(["aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into()]),
            ..FakeRunner::default()
        };
        let manifest = build_manifest_with_runner(
            temp.path(),
            &dist,
            &crate_source(),
            Some("v0.9.0"),
            &mut runner,
        )?;
        let paths = write_manifest_outputs(
            temp.path(),
            &dist,
            &manifest,
            Path::new("generated/release-manifest.json"),
            Path::new("generated/SHA256SUMS"),
        )?;

        assert!(fs::read_to_string(&paths.manifest)?.ends_with('\n'));
        let checksums = fs::read_to_string(&paths.checksums)?;
        assert!(checksums.ends_with('\n'));
        let manifest_digest = sha256_file(&paths.manifest)?;
        assert!(checksums.contains(&format!("{manifest_digest}  release-manifest.json\n")));
        Ok(())
    }

    #[test]
    fn output_paths_cannot_collide_with_each_other_or_source_archives() -> Result<()> {
        let (temp, dist) = fixture()?;
        let mut runner = FakeRunner {
            outputs: VecDeque::from(["aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into()]),
            ..FakeRunner::default()
        };
        let manifest = build_manifest_with_runner(
            temp.path(),
            &dist,
            &crate_source(),
            Some("v0.9.0"),
            &mut runner,
        )?;
        let shared = Path::new("generated/release-metadata");
        let error =
            write_manifest_outputs(temp.path(), &dist, &manifest, shared, shared).unwrap_err();
        assert!(error.to_string().contains("must be different paths"));
        assert!(!temp.path().join(shared).exists());

        let archive = dist.join(&manifest.artifacts[0].name);
        let original = fs::read(&archive)?;
        let error = write_manifest_outputs(
            temp.path(),
            &dist,
            &manifest,
            &archive,
            Path::new("generated/SHA256SUMS"),
        )
        .unwrap_err();
        assert!(
            error
                .to_string()
                .contains("must not overwrite source archive")
        );
        assert_eq!(fs::read(archive)?, original);
        Ok(())
    }
