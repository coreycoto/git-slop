fn validate_marketplaces(repo_root: &Path, errors: &mut Vec<String>) {
    runtime_manifest::validate_marketplace_source(repo_root, errors);

    if let Some(marketplace) = load_json(repo_root, GIT_SLOP_MARKETPLACE, errors) {
        if json_string(&marketplace, "name") != Some(GIT_SLOP_MARKETPLACE_NAME) {
            errors
                .push(".agents/plugins/marketplace.json must define git-slop-marketplace.".into());
        }
        if marketplace
            .pointer("/interface/displayName")
            .and_then(JsonValue::as_str)
            != Some("Git Slop Agent Plugin")
        {
            errors.push(
                ".agents/plugins/marketplace.json must display Git Slop Agent Plugin.".into(),
            );
        }
        let expected_plugins = json!([
            {
                "name": "git-slop",
                "source": {"source": "local", "path": "./plugins/git-slop"},
                "policy": {
                    "installation": "AVAILABLE",
                    "authentication": "ON_INSTALL"
                },
                "category": "Developer Tools"
            }
        ]);
        if marketplace.get("plugins") != Some(&expected_plugins) {
            errors.push(
                ".agents/plugins/marketplace.json must publish exactly the expected git-slop \
                 product plugin entry."
                    .into(),
            );
        }
    }
}

fn validate_product_plugin(repo_root: &Path, errors: &mut Vec<String>) {
    let codex_compatibility_manifest = load_json(repo_root, GIT_SLOP_CODEX_COMPAT_MANIFEST, errors);
    let project_version = match project_version(repo_root) {
        Ok(version) => Some(version),
        Err(error) => {
            errors.push(format!(
                "unable to resolve the git-slop package version for Agent Plugin validation: {error}"
            ));
            None
        }
    };

    if let Some(manifest) = load_json(repo_root, GIT_SLOP_PLUGIN_MANIFEST, errors) {
        let expected_keys = [
            "$schema",
            "author",
            "description",
            "extensions",
            "homepage",
            "keywords",
            "license",
            "name",
            "repository",
            "version",
        ]
        .into_iter()
        .collect::<BTreeSet<_>>();
        match manifest.as_object() {
            Some(object) => {
                let actual_keys = object.keys().map(String::as_str).collect::<BTreeSet<_>>();
                if actual_keys != expected_keys {
                    errors.push(format!(
                        "{GIT_SLOP_PLUGIN_MANIFEST} must define exactly the portable Agent Plugins fields; found {actual_keys:?}."
                    ));
                }
            }
            None => errors.push(format!(
                "{GIT_SLOP_PLUGIN_MANIFEST} must contain a JSON object."
            )),
        }
        if json_string(&manifest, "$schema") != Some(AGENT_PLUGIN_SCHEMA) {
            errors.push(format!(
                "git-slop Agent Plugin manifest must target {AGENT_PLUGIN_SCHEMA}."
            ));
        }
        if json_string(&manifest, "name") != Some(GIT_SLOP_PLUGIN_NAME) {
            errors.push("git-slop Agent Plugin manifest must use name git-slop.".into());
        }
        if let Some(project_version) = project_version.as_deref()
            && json_string(&manifest, "version") != Some(project_version)
        {
            errors.push(format!(
                "git-slop Agent Plugin manifest version must match Cargo.toml package.version \
                 {project_version}."
            ));
        }
        let expected_portable_metadata = json!({
            "description": "Agent guidance for the deterministic git-slop repository token-defragmenter.",
            "author": {
                "name": "Corey Coto",
                "email": "support@coreycoto.com",
                "url": "https://github.com/coreycoto"
            },
            "homepage": "https://github.com/coreycoto/git-slop/tree/main/plugins/git-slop",
            "repository": "https://github.com/coreycoto/git-slop",
            "license": "MIT",
            "keywords": [
                "agent-plugin",
                "agent-skills",
                "codex",
                "cursor",
                "git-slop",
                "github-copilot",
                "kiro",
                "repo-health",
                "hotspots",
                "vscode"
            ]
        });
        for key in [
            "description",
            "author",
            "homepage",
            "repository",
            "license",
            "keywords",
        ] {
            if manifest.get(key) != expected_portable_metadata.get(key) {
                errors.push(format!(
                    "git-slop Agent Plugin manifest must preserve the expected portable {key} metadata."
                ));
            }
        }

        let expected_openai_extension = json!({
            "interface": {
                "displayName": "Git Slop",
                "shortDescription": "Defragment repository context deterministically",
                "longDescription": "Agent guidance for installing or updating the native git-slop repository token-defragmenter, running deterministic context-cost and repository-health reports, ratcheting pull requests against compatible baselines, interpreting evidence, and planning bounded maintenance slices.",
                "developerName": "Corey Coto",
                "category": "Developer Tools",
                "capabilities": ["Interactive", "Read"],
                "websiteURL": "https://github.com/coreycoto/git-slop/tree/main/plugins/git-slop",
                "privacyPolicyURL": "https://github.com/coreycoto/git-slop",
                "termsOfServiceURL": "https://github.com/coreycoto/git-slop",
                "defaultPrompt": [
                    "Use git-slop guidance to install, run, automate, and interpret repository-health reports.",
                    "Keep git-slop as an observational detector unless the repository explicitly promotes its results into required gates.",
                    "Reference project-management workflow skills only when converting reviewed git-slop maintenance plans into backlog work."
                ],
                "brandColor": GIT_SLOP_BRAND_COLOR,
                "composerIcon": "./assets/git-slop.svg",
                "logo": "./assets/git-slop.svg",
                "screenshots": ["./assets/repository-health.png"]
            }
        });
        if manifest.pointer("/extensions/com.openai") != Some(&expected_openai_extension) {
            errors.push(
                "git-slop Agent Plugin manifest must isolate the expected Codex UI metadata under extensions.com.openai."
                    .into(),
            );
        }
        if manifest
            .get("extensions")
            .and_then(JsonValue::as_object)
            .is_none_or(|extensions| {
                extensions.len() != 1 || !extensions.contains_key("com.openai")
            })
        {
            errors.push(
                "git-slop Agent Plugin manifest must declare only the com.openai client extension."
                    .into(),
            );
        }

        if let Some(codex_compatibility_manifest) = codex_compatibility_manifest {
            let expected_codex_compatibility_manifest = json!({
                "name": manifest["name"].clone(),
                "version": manifest["version"].clone(),
                "description": manifest["description"].clone(),
                "author": manifest["author"].clone(),
                "homepage": manifest["homepage"].clone(),
                "repository": manifest["repository"].clone(),
                "license": manifest["license"].clone(),
                "keywords": manifest["keywords"].clone(),
                "interface": manifest
                    .pointer("/extensions/com.openai/interface")
                    .cloned()
                    .unwrap_or(JsonValue::Null)
            });
            if codex_compatibility_manifest != expected_codex_compatibility_manifest {
                errors.push(format!(
                    "{GIT_SLOP_CODEX_COMPAT_MANIFEST} must be an exact metadata-only mirror of \
                     {GIT_SLOP_PLUGIN_MANIFEST} for Codex 0.146.x compatibility; do not declare \
                     skills or other plugin components in the overlay."
                ));
            }
        }

        for asset in ["assets/git-slop.svg"] {
            if !repo_root.join(GIT_SLOP_PLUGIN_ROOT).join(asset).is_file() {
                errors.push(format!(
                    "git-slop Agent Plugin referenced asset is missing: {asset}."
                ));
            }
        }
    }

    let skills_root = repo_root.join(GIT_SLOP_PLUGIN_ROOT).join("skills");
    let expected = GIT_SLOP_PLUGIN_SKILLS.into_iter().collect::<BTreeSet<_>>();
    let mut actual = BTreeSet::new();
    match fs::read_dir(&skills_root) {
        Ok(entries) => {
            for entry in entries {
                let entry = match entry {
                    Ok(entry) => entry,
                    Err(error) => {
                        errors.push(format!(
                            "Unable to inspect plugins/git-slop/skills: {error}"
                        ));
                        continue;
                    }
                };
                let path = entry.path();
                if path.join("SKILL.md").is_file() {
                    if let Some(name) = entry.file_name().to_str() {
                        actual.insert(name.to_owned());
                    }
                }
            }
        }
        Err(error) => errors.push(format!(
            "Unable to inspect plugins/git-slop/skills: {error}"
        )),
    }

    for skill_name in expected.difference(&actual.iter().map(String::as_str).collect()) {
        errors.push(format!("plugins/git-slop skill is missing: {skill_name}."));
    }
    let actual_refs = actual.iter().map(String::as_str).collect::<BTreeSet<_>>();
    if actual_refs != expected {
        errors.push(format!(
            "plugins/git-slop must expose exactly the expected skill directories; found {actual:?}."
        ));
    }

    for skill_name in GIT_SLOP_PLUGIN_SKILLS {
        let relative = format!("{GIT_SLOP_PLUGIN_ROOT}/skills/{skill_name}/SKILL.md");
        let Some(text) = read_text(repo_root, &relative, errors) else {
            continue;
        };
        let mut sections = text.splitn(3, "---");
        let frontmatter = match (sections.next(), sections.next(), sections.next()) {
            (Some(""), Some(frontmatter), Some(_)) => frontmatter,
            _ => {
                errors.push(format!(
                    "{relative} must begin with Agent Skills YAML frontmatter."
                ));
                continue;
            }
        };
        let metadata = match serde_yaml::from_str::<serde_yaml::Value>(frontmatter) {
            Ok(metadata) => metadata,
            Err(error) => {
                errors.push(format!("Unable to parse {relative} frontmatter: {error}"));
                continue;
            }
        };
        if metadata.get("name").and_then(serde_yaml::Value::as_str) != Some(skill_name) {
            errors.push(format!(
                "{relative} frontmatter name must match its skill directory."
            ));
        }
        let description = metadata
            .get("description")
            .and_then(serde_yaml::Value::as_str);
        if description.is_none_or(|description| description.trim().is_empty()) {
            errors.push(format!(
                "{relative} frontmatter must include a non-empty description."
            ));
        }
        let expected_description = GIT_SLOP_SKILL_CONTRACTS
            .iter()
            .find(|presentation| presentation.skill_name == skill_name)
            .map(|presentation| presentation.trigger_description);
        if description != expected_description {
            errors.push(format!(
                "{relative} frontmatter must preserve its expected implicit-invocation trigger description."
            ));
        }
    }

    let root_icon = read_text(repo_root, GIT_SLOP_ICON, errors);
    if let Some(root_icon) = root_icon.as_deref() {
        if !root_icon.contains(r#"width="64""#)
            || !root_icon.contains(r#"height="64""#)
            || !root_icon.contains(r#"viewBox="0 0 24 24""#)
        {
            errors.push(format!(
                "{GIT_SLOP_ICON} must declare 64x64 dimensions and preserve viewBox 0 0 24 24 for OpenAI directory branding."
            ));
        }
        if root_icon.contains(" color=") || !root_icon.contains(r#"fill="currentColor""#) {
            errors.push(format!(
                "{GIT_SLOP_ICON} must inherit its foreground color so the Git Slop glyph remains legible on the {GIT_SLOP_BRAND_COLOR} brand background."
            ));
        }
    }
    for contract in GIT_SLOP_SKILL_CONTRACTS {
        let skill_root = format!("{GIT_SLOP_PLUGIN_ROOT}/skills/{}", contract.skill_name);
        let agents_root = repo_root.join(&skill_root).join("agents");
        let mut actual_agent_entries = BTreeSet::new();
        match fs::read_dir(&agents_root) {
            Ok(entries) => {
                for entry in entries {
                    match entry {
                        Ok(entry) => {
                            if let Some(name) = entry.file_name().to_str() {
                                actual_agent_entries.insert(name.to_owned());
                            }
                        }
                        Err(error) => {
                            errors.push(format!("Unable to inspect {skill_root}/agents: {error}"))
                        }
                    }
                }
            }
            Err(error) => errors.push(format!("Unable to inspect {skill_root}/agents: {error}")),
        }
        if actual_agent_entries != BTreeSet::from(["openai.yaml".to_owned()]) {
            errors.push(format!(
                "{skill_root}/agents must contain only the optional OpenAI presentation file openai.yaml; found {actual_agent_entries:?}."
            ));
        }

        let openai_relative = format!("{skill_root}/agents/openai.yaml");
        if let Some(text) = read_text(repo_root, &openai_relative, errors) {
            let expected = format!(
                "interface:\n  display_name: {:?}\n  short_description: {:?}\n  icon_small: \"./assets/git-slop.svg\"\n  icon_large: \"./assets/git-slop.svg\"\n  brand_color: {GIT_SLOP_BRAND_COLOR:?}\n  default_prompt: {:?}\n\npolicy:\n  allow_implicit_invocation: true\n",
                contract.display_name, contract.short_description, contract.default_prompt,
            );
            match (
                serde_yaml::from_str::<serde_yaml::Value>(&text),
                serde_yaml::from_str::<serde_yaml::Value>(&expected),
            ) {
                (Ok(actual), Ok(expected)) if actual == expected => {}
                (Ok(_), Ok(_)) => errors.push(format!(
                    "{openai_relative} must preserve the expected OpenAI presentation contract with policy.allow_implicit_invocation set to true."
                )),
                (Err(error), _) => errors.push(format!(
                    "Unable to parse {openai_relative} as YAML: {error}"
                )),
                (_, Err(error)) => errors.push(format!(
                    "Unable to construct expected OpenAI metadata for {}: {error}",
                    contract.skill_name
                )),
            }
        }

        let skill_icon_relative = format!("{skill_root}/assets/git-slop.svg");
        match fs::read(repo_root.join(&skill_icon_relative)) {
            Ok(skill_icon) => {
                if root_icon
                    .as_deref()
                    .is_some_and(|root_icon| root_icon.as_bytes() != skill_icon)
                {
                    errors.push(format!(
                        "{skill_icon_relative} must be the same Git Slop icon as plugins/git-slop/assets/git-slop.svg."
                    ));
                }
            }
            Err(error) => errors.push(format!("Unable to read {skill_icon_relative}: {error}")),
        }
    }
}
