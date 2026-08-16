    #[test]
    fn strict_semver_matches_stable_contract() {
        for valid in ["0.0.0", "0.9.0", "10.20.300"] {
            assert!(is_strict_semver(valid), "{valid}");
        }
        for invalid in [
            "v0.9.0",
            "01.2.3",
            "1.02.3",
            "1.2.03",
            "1.2",
            "1.2.3.4",
            "1.2.3-rc.1",
            "1.a.3",
            "",
        ] {
            assert!(!is_strict_semver(invalid), "{invalid}");
        }
    }

    #[test]
    fn immutable_v0_12_0_keeps_its_tagged_action_install_contract() {
        let legacy = install_instructions("v0.12.0");
        assert_eq!(legacy.attestation.len(), 1);
        assert!(!legacy.github_release[0].starts_with("gh release verify "));

        let hardened = install_instructions("v0.12.1");
        assert_eq!(hardened.attestation.len(), RELEASE_TARGETS.len());
        assert_eq!(
            hardened.github_release[0],
            "gh release verify v0.12.1 --repo coreycoto/git-slop"
        );
    }

    #[test]
    fn exact_release_matrix_builds_deterministic_manifest() -> Result<()> {
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

        assert_eq!(manifest.schema_version, 3);
        assert_eq!(manifest.project, PROJECT_NAME);
        assert_eq!(manifest.repository, REPO_FULL_NAME);
        assert_eq!(manifest.artifacts.len(), RELEASE_TARGETS.len());
        assert_eq!(
            manifest
                .artifacts
                .iter()
                .map(|artifact| artifact.target.as_str())
                .collect::<Vec<_>>(),
            RELEASE_TARGETS
                .iter()
                .map(|target| target.target)
                .collect::<Vec<_>>()
        );
        assert!(manifest.artifacts.iter().all(|artifact| {
            artifact.url
                == format!(
                    "https://github.com/{REPO_FULL_NAME}/releases/download/v0.9.0/{}",
                    artifact.name
                )
        }));
        assert_eq!(runner.output_calls.len(), 1);
        assert_eq!(
            runner.output_calls[0].1,
            CommandSpec::new(
                "git",
                ["rev-parse", "--verify", "refs/tags/v0.9.0^{commit}",]
            )
        );

        let checksums = checksum_lines(&manifest.artifacts);
        assert_eq!(
            checksums,
            include_str!("../../../tests/fixtures/SHA256SUMS-v0.9.0")
        );

        let json = render_manifest_json(&manifest)?;
        assert_eq!(
            json,
            include_str!("../../../tests/fixtures/release-manifest-v0.9.0.json")
        );
        assert_eq!(json, render_manifest_json(&manifest)?);
        Ok(())
    }

    #[test]
    fn supplemental_asset_roles_drive_the_complete_published_inventory() -> Result<()> {
        let (temp, dist) = fixture()?;
        for (name, _, _) in SUPPLEMENTAL_RELEASE_ASSETS {
            fs::write(dist.join(name), format!("fixture for {name}\n"))?;
        }
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

        assert_eq!(
            manifest
                .supplemental_assets
                .iter()
                .map(|asset| asset.role.as_str())
                .collect::<Vec<_>>(),
            vec!["homebrew_formula", "cyclonedx_sbom", "spdx_sbom"]
        );
        assert!(
            manifest
                .supplemental_assets
                .iter()
                .all(|asset| asset.required && asset.contract_version == 1)
        );
        let rendered = render_manifest_json(&manifest)?;
        let output = temp.path().join("dist/release-manifest.json");
        fs::write(&output, &rendered)?;
        let checksums = checksum_lines_with_manifest(
            &manifest.artifacts,
            &manifest.supplemental_assets,
            "release-manifest.json",
            &sha256_file(&output)?,
        );
        assert_eq!(checksums.lines().count(), 11);
        for (name, _, _) in SUPPLEMENTAL_RELEASE_ASSETS {
            assert!(checksums.lines().any(|line| line.ends_with(name)));
        }
        Ok(())
    }
