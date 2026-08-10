fn validate_candidate_distribution_job(candidate_distribution: Option<&YamlValue>, errors: &mut Vec<String>) {
    let name = "release-publish.yml";
    if let Some(candidate_distribution) = candidate_distribution {
        require_needs(
            candidate_distribution,
            name,
            "candidate-distribution",
            &["candidate", "candidate-targets"],
            errors,
        );
        let Some(run) = step_run(
            candidate_distribution,
            "Dry-run release manifest and crates-backed Formula",
        ) else {
            errors.push(format!(
                "{name} candidate-distribution must dry-run manifest and Formula generation."
            ));
            return;
        };
        require(
            run,
            &format!(r#"-c user.email="{RELEASE_VALIDATION_EMAIL}""#),
            name,
            errors,
        );
        for required in [
            r#"-c user.name="git-slop release validation""#,
            "cargo xtask release-manifest",
            "--crate-source candidate/crate-source.json",
            "cargo xtask homebrew-formula",
            "cargo xtask sbom --output-dir candidate-dist",
            "sha256sum --check SHA256SUMS",
            "wc -l < candidate-dist/SHA256SUMS",
            "= \"11\"",
        ] {
            require(run, required, name, errors);
        }
        if run.matches("cargo xtask release-manifest").count() != 2 {
            errors.push(format!(
                "{name} candidate distribution must regenerate the manifest after Formula and SBOM generation."
            ));
        }
        forbid(
            run,
            "sha256sum git-slop.rb release-manifest.json",
            name,
            errors,
        );
        let upload = named_step(
            candidate_distribution,
            "Upload candidate Formula for Homebrew audit",
        );
        let upload_valid = upload.is_some_and(|step| {
            step.get("uses").and_then(YamlValue::as_str)
                == Some("actions/upload-artifact@043fb46d1a93c77aae656e7c1c64a875d1fc6a0a")
                && step
                    .get("with")
                    .and_then(|with| with.get("name"))
                    .and_then(YamlValue::as_str)
                    == Some("candidate-homebrew-formula")
                && step
                    .get("with")
                    .and_then(|with| with.get("path"))
                    .and_then(YamlValue::as_str)
                    == Some("candidate-dist/git-slop.rb")
                && step
                    .get("with")
                    .and_then(|with| with.get("if-no-files-found"))
                    .and_then(YamlValue::as_str)
                    == Some("error")
                && step
                    .get("with")
                    .and_then(|with| with.get("retention-days"))
                    .and_then(YamlValue::as_u64)
                    == Some(1)
        });
        if !upload_valid {
            errors.push(format!(
                "{name} candidate-distribution must upload only the generated Formula with the pinned bounded artifact contract."
            ));
        }
    }

}
