fn validate_candidate_targets_job(candidate_targets: Option<&YamlValue>, text: &str, errors: &mut Vec<String>) {
    let name = "release-publish.yml";
    if let Some(candidate_targets) = candidate_targets {
        require_needs(
            candidate_targets,
            name,
            "candidate-targets",
            &["candidate"],
            errors,
        );
        validate_target_matrix(candidate_targets, name, "candidate-targets", true, errors);
        if text
            .matches("candidate_source_dir=\"${RUNNER_TEMP}/candidate-source\"")
            .count()
            != 3
            || text
                .matches(
                    "$candidateSourceDir = Join-Path $env:RUNNER_TEMP \"candidate-source\"",
                )
                .count()
                != 2
        {
            errors.push(format!(
                "{name} candidate-targets must unpack candidate source outside the repository workspace."
            ));
        }
        for required in [
            "Download exact candidate package",
            "Verify and unpack candidate bytes",
            "Verify and unpack candidate bytes on Windows",
            "Get-FileHash -Algorithm SHA256 $crate",
            "tar -xzf \"$crate\" -C \"$candidate_source_dir\"",
            "package=\"${candidate_source_dir}/git-slop-${VERSION}\"",
            "Join-Path $candidateSourceDir \"git-slop-$env:VERSION\"",
            "build-info --format json",
            ".source_dirty == false",
        ] {
            require(text, required, name, errors);
        }
    }

}
