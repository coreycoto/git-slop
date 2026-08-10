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
        for required in [
            "Download exact candidate package",
            "Verify and unpack candidate bytes",
            "build-info --format json",
            ".source_dirty == false",
        ] {
            require(text, required, name, errors);
        }
    }

}
