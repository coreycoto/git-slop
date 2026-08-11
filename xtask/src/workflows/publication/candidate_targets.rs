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
        if candidate_targets
            .get("env")
            .and_then(|env| env.get("CANDIDATE_SOURCE_DIR"))
            .and_then(YamlValue::as_str)
            != Some("${{ runner.temp }}/candidate-source")
        {
            errors.push(format!(
                "{name} candidate-targets must unpack candidate source outside the repository workspace."
            ));
        }
        for required in [
            "Download exact candidate package",
            "Verify and unpack candidate bytes",
            "tar -xzf \"$crate\" -C \"$CANDIDATE_SOURCE_DIR\"",
            "package=\"${CANDIDATE_SOURCE_DIR}/git-slop-${VERSION}\"",
            "Join-Path $env:CANDIDATE_SOURCE_DIR \"git-slop-$env:VERSION\"",
            "build-info --format json",
            ".source_dirty == false",
        ] {
            require(text, required, name, errors);
        }
    }

}
