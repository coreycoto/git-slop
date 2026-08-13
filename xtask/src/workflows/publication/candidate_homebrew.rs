fn validate_candidate_homebrew_job(candidate_homebrew_audit: Option<&YamlValue>, errors: &mut Vec<String>) {
    let name = "release-publish.yml";
    if let Some(candidate_homebrew_audit) = candidate_homebrew_audit {
        require_needs(
            candidate_homebrew_audit,
            name,
            "candidate-homebrew-audit",
            &["candidate-distribution"],
            errors,
        );
        if candidate_homebrew_audit
            .get("runs-on")
            .and_then(YamlValue::as_str)
            != Some("macos-26")
        {
            errors.push(format!(
                "{name} candidate-homebrew-audit must run with native Homebrew on macos-26."
            ));
        }
        let Some(run) = step_run(
            candidate_homebrew_audit,
            "Audit candidate Formula with Homebrew",
        ) else {
            errors.push(format!(
                "{name} candidate-homebrew-audit must run the Homebrew audit gate."
            ));
            return;
        };
        for required in [
            "brew tap-new --no-git",
            "brew audit --strict --formula",
            "brew style --formula",
        ] {
            require(run, required, name, errors);
        }
        let setup_action = named_step(candidate_homebrew_audit, "Set up Homebrew")
            .and_then(|step| step.get("uses"))
            .and_then(YamlValue::as_str);
        if !setup_action
            .is_some_and(|action| action.starts_with("Homebrew/actions/setup-homebrew@"))
        {
            errors.push(format!(
                "{name} candidate-homebrew-audit must use the Homebrew setup Action."
            ));
        }
        let download = named_step(candidate_homebrew_audit, "Download candidate Formula");
        let download_valid = download.is_some_and(|step| {
            step.get("uses").and_then(YamlValue::as_str)
                == Some("actions/download-artifact@3e5f45b2cfb9172054b4087a40e8e0b5a5461e7c")
                && step
                    .get("with")
                    .and_then(|with| with.get("name"))
                    .and_then(YamlValue::as_str)
                    == Some("candidate-homebrew-formula")
                && step
                    .get("with")
                    .and_then(|with| with.get("path"))
                    .and_then(YamlValue::as_str)
                    == Some("${{ runner.temp }}/candidate-homebrew")
        });
        if !download_valid {
            errors.push(format!(
                "{name} candidate-homebrew-audit must download only the generated Formula with the pinned artifact contract."
            ));
        }
        let audit_formula_path = named_step(
            candidate_homebrew_audit,
            "Audit candidate Formula with Homebrew",
        )
        .and_then(|step| step_env(step, "FORMULA_PATH"));
        if audit_formula_path != Some("${{ runner.temp }}/candidate-homebrew/git-slop.rb") {
            errors.push(format!(
                "{name} candidate-homebrew-audit must audit the exact downloaded Formula path."
            ));
        }
    }

}
