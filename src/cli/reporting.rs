fn run_report(args: ReportArgs) -> Result<i32> {
    match args.command {
        ReportCommand::Validate { path, report, allow_legacy, format } => {
            let path = path.or(report).expect("Clap requires a report path");
            match report::load_report_with_legacy(&path, allow_legacy) {
                Ok(value) => {
                    let payload = json!({
                        "schema_version": 1,
                        "command": "report validate",
                        "valid": true,
                        "report_schema_version": value["schema_version"],
                        "report": portable_source(&path)
                    });
                    match format {
                        DisplayFormat::Text => println!(
                            "Report is valid: {} (schema {}).",
                            path.display(),
                            value["schema_version"]
                        ),
                        DisplayFormat::Json => print_text(&render_json(&payload)?),
                        DisplayFormat::Yaml => print_text(&serde_yaml::to_string(&payload)?),
                    }
                    Ok(0)
                }
                Err(error) => {
                    let violations = fs::read_to_string(&path)
                        .ok()
                        .and_then(|source| serde_json::from_str::<Value>(&source).ok())
                        .map(|report| report::validation_violations(&report))
                        .unwrap_or_default();
                    Err(ClassifiedError::new(
                        ErrorKind::Contract,
                        "report_invalid",
                        format!("{error:#}"),
                    )
                    .at("/report")
                    .with_details(json!({"path": path, "violations": violations}))
                    .into())
                }
            }
        }
        ReportCommand::Migrate { path, output } => {
            let source = fs::read_to_string(&path)
                .with_context(|| format!("failed to read {}", path.display()))?;
            let value: Value = serde_json::from_str(&source)
                .with_context(|| format!("invalid git-slop report JSON: {}", path.display()))?;
            let migrated = report::migrate_legacy_report(value)?;
            report::write_json_atomically(&output, &migrated)?;
            println!(
                "Migrated {} to schema 5 at {}.",
                path.display(),
                output.display()
            );
            Ok(0)
        }
        ReportCommand::Schema => {
            print_text(&render_json(&report::schema())?);
            Ok(0)
        }
    }
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum SarifScope {
    Policy,
    ActionQueue,
}

fn run_sarif(repo_root: &Path, args: SarifArgs) -> Result<i32> {
    let (loaded, report_path) = report_or_missing(repo_root, args.report.as_deref())?;
    let top = match args.top {
        None => None,
        Some(value) => match usize::try_from(value).ok().filter(|count| *count > 0) {
            Some(value) => Some(value),
            None => return argument_error("/top", "--top", "--top must be greater than zero.", value),
        },
    };
    let report_descriptor = args
        .include_local_paths
        .then(|| report_path.to_string_lossy().to_string());
    let scope = match args.scope {
        SarifScope::Policy => "policy",
        SarifScope::ActionQueue => "action-queue",
    };
    let payload = match sarif_payload(&loaded, report_descriptor.as_deref(), top, scope) {
        Ok(payload) => payload,
        Err(error) => return usage_error(error),
    };
    let rendered = render_json(&payload)?;
    if let Some(output) = args.output {
        if let Some(parent) = output
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }
        config::write_text_atomically(&output, rendered, false)
            .with_context(|| format!("failed to write {}", output.display()))?;
        println!("Wrote SARIF report to {}.", output.display());
    } else {
        print_text(&rendered);
    }
    Ok(0)
}

fn run_health(repo_root: &Path, args: HealthArgs) -> Result<i32> {
    let (mut loaded, _) = report_or_missing(repo_root, args.report.as_deref())?;
    let freshness = if args.require_current {
        Some(require_current_report(repo_root, &loaded)?)
    } else if !repo_root.as_os_str().is_empty() {
        crate::freshness::evaluate(repo_root, &loaded).ok()
    } else {
        None
    };
    let rollup = match health::health_rollup_from_report(&loaded) {
        Ok(rollup) => rollup,
        Err(error) => return usage_error(error),
    };
    let mut health_value = serde_json::to_value(rollup)?;
    if let (Some(existing), Some(derived)) = (
        loaded.get("health").and_then(Value::as_object),
        health_value.as_object_mut(),
    ) {
        for (key, value) in existing {
            derived.entry(key.clone()).or_insert_with(|| value.clone());
        }
    }
    if let Some(object) = loaded.as_object_mut() {
        object.insert("health".to_string(), health_value);
    }
    match args.format {
        HealthFormat::Text => print_text(&report::render_terminal(&loaded)),
        HealthFormat::Markdown => {
            let rendered = match health::render_health_from_report(&loaded) {
                Ok(rendered) => rendered,
                Err(error) => return usage_error(error),
            };
            print_text(&rendered);
        }
        HealthFormat::Github => {
            if let Some(freshness) = freshness.as_ref().filter(|value| !value.current) {
                println!(
                    "::warning::Git Slop report is stale ({})",
                    crate::text::github_property_escape(&freshness.reason_codes())
                );
            }
            print_text(&render_github_annotations(&loaded, args.max_annotations));
        }
        HealthFormat::Json => {
            let mut payload = health_json_payload(&loaded);
            payload["freshness"] = serde_json::to_value(freshness)?;
            print_text(&render_json(&payload)?);
        }
    }
    Ok(0)
}

fn diff_values(current: &Value, defaults: &Value) -> Value {
    match (current, defaults) {
        (Value::Object(current), Value::Object(defaults)) => {
            let mut result = serde_json::Map::new();
            for (key, value) in current {
                if matches!(key.as_str(), "tokenizer" | "context_bands") {
                    continue;
                }
                let difference = defaults
                    .get(key)
                    .map_or_else(|| value.clone(), |default| diff_values(value, default));
                if !difference.is_null()
                    && !difference
                        .as_object()
                        .is_some_and(serde_json::Map::is_empty)
                {
                    result.insert(key.clone(), difference);
                }
            }
            Value::Object(result)
        }
        _ if current == defaults => Value::Null,
        _ => current.clone(),
    }
}

fn load_config_contract(repo_root: &Path) -> Result<Value> {
    config::load(repo_root).map_err(|error| {
        let message = format!("{error:#}");
        let pointer = message
            .split_whitespace()
            .map(|token| token.trim_matches(|character: char| !character.is_ascii_alphanumeric() && character != '.' && character != '_' && character != '[' && character != ']'))
            .find(|token| token.contains('.') && !token.ends_with(".yaml"))
            .map(|token| format!("/{}", token.replace('.', "/")))
            .unwrap_or_else(|| "/config".to_string());
        ClassifiedError::new(
            ErrorKind::Contract,
            "invalid_configuration",
            message,
        )
        .at(pointer)
        .with_details(json!({"config_path": config::config_path(repo_root)}))
        .into()
    })
}

fn run_config(repo_root: &Path, args: ConfigArgs) -> Result<i32> {
    match args.command {
        ConfigCommand::Show { effective } => {
            if effective {
                print_text(&serde_yaml::to_string(&load_config_contract(repo_root)?)?);
            } else {
                let path = config::config_path(repo_root);
                if path.exists() {
                    print_text(&fs::read_to_string(path)?);
                } else {
                    print_text(config::MINIMAL_CONFIG);
                }
            }
        }
        ConfigCommand::Validate => {
            load_config_contract(repo_root)?;
            let path = config::config_path(repo_root);
            if path.exists() {
                println!("Configuration is valid: {}", path.display());
            } else {
                println!(
                    "Configuration is valid: built-in defaults ({} is absent).",
                    path.display()
                );
            }
        }
        ConfigCommand::DiffDefaults => {
            let diff = diff_values(&load_config_contract(repo_root)?, &config::default_config());
            print_text(&serde_yaml::to_string(&diff)?);
        }
        ConfigCommand::Migrate { dry_run, no_backup } => {
            let effective = load_config_contract(repo_root)?;
            let mut diff = diff_values(&effective, &config::default_config());
            if let Some(object) = diff.as_object_mut() {
                object.insert("schema_version".into(), json!(2));
            }
            let rendered = serde_yaml::to_string(&diff)?;
            if dry_run {
                println!(
                    "# Preview only; {} was not changed.",
                    config::config_path(repo_root).display()
                );
                print_text(&rendered);
                return Ok(0);
            }
            config::ensure_state_dirs(repo_root)?;
            let backup = config::write_text_atomically(
                &config::config_path(repo_root),
                rendered,
                !no_backup,
            )?;
            println!(
                "Migrated {} to schema 2.",
                config::config_path(repo_root).display()
            );
            if let Some(backup) = backup {
                println!("Recovery backup: {}.", backup.display());
            }
        }
        ConfigCommand::Schema => print_text(&render_json(&config::schema())?),
    }
    Ok(0)
}
