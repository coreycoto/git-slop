fn validate_build_job(build: Option<&YamlValue>, text: &str, errors: &mut Vec<String>) {
    let name = "release-publish.yml";
    if let Some(build) = build {
        require_needs(build, name, "build", &["publish-crate"], errors);
        validate_target_matrix(build, name, "build", false, errors);
        let rendered_steps = steps(build)
            .into_iter()
            .filter_map(|step| step.get("uses").and_then(YamlValue::as_str))
            .collect::<Vec<_>>();
        if rendered_steps
            .iter()
            .any(|uses| uses.starts_with("actions/checkout@"))
        {
            errors.push(format!(
                "{name} build must compile only the verified registry crate, without a source checkout."
            ));
        }
        for required in [
            "name: verified-registry-crate",
            "registry-source/git-slop-${VERSION}",
            "cargo build --manifest-path \"$package/Cargo.toml\"",
            "build-info --format json",
        ] {
            require(text, required, name, errors);
        }
    }

}
