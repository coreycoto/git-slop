#[derive(Debug, Clone)]
pub struct InitResult {
    pub config: String,
    pub gitignore: String,
    pub backups: Vec<PathBuf>,
}

#[derive(Debug, Clone)]
pub struct AdoptionStatus {
    pub config_exists: bool,
    pub config_valid: bool,
    pub gitignore_exists: bool,
    pub missing_ignore_entries: Vec<String>,
}

impl AdoptionStatus {
    pub fn ready(&self) -> bool {
        self.config_exists
            && self.config_valid
            && self.gitignore_exists
            && self.missing_ignore_entries.is_empty()
    }
}

fn required_ignore_entries() -> Vec<&'static str> {
    DEFAULT_SLOP_GITIGNORE
        .lines()
        .filter(|line| !line.trim().is_empty())
        .collect()
}

pub fn adoption_status(repo_root: &Path) -> AdoptionStatus {
    let config_target = config_path(repo_root);
    let gitignore_target = slop_dir(repo_root).join(".gitignore");
    let gitignore = fs::read_to_string(&gitignore_target).unwrap_or_default();
    let present = gitignore
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<std::collections::BTreeSet<_>>();
    AdoptionStatus {
        config_exists: config_target.is_file(),
        config_valid: config_target.is_file() && load(repo_root).is_ok(),
        gitignore_exists: gitignore_target.is_file(),
        missing_ignore_entries: required_ignore_entries()
            .into_iter()
            .filter(|entry| !present.contains(entry))
            .map(ToOwned::to_owned)
            .collect(),
    }
}

pub fn initialize(
    repo_root: &Path,
    force: bool,
    repair: bool,
    gitignore_only: bool,
) -> Result<InitResult> {
    ensure_state_dirs(repo_root)?;
    let mut backups = Vec::new();
    let config_target = config_path(repo_root);
    let config_status = if gitignore_only {
        "skipped".to_string()
    } else if force || !config_target.exists() {
        if let Some(backup) = write_text_atomically(&config_target, MINIMAL_CONFIG, force)? {
            backups.push(backup);
        }
        if force { "replaced" } else { "written" }.to_string()
    } else {
        "kept".to_string()
    };
    let gitignore_target = slop_dir(repo_root).join(".gitignore");
    let gitignore_status = if force || !gitignore_target.exists() {
        if let Some(backup) =
            write_text_atomically(&gitignore_target, DEFAULT_SLOP_GITIGNORE, force)?
        {
            backups.push(backup);
        }
        if force { "replaced" } else { "written" }.to_string()
    } else if repair {
        let current = fs::read_to_string(&gitignore_target)?;
        let present = current
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .collect::<std::collections::BTreeSet<_>>();
        let missing = required_ignore_entries()
            .into_iter()
            .filter(|entry| !present.contains(entry))
            .collect::<Vec<_>>();
        if missing.is_empty() {
            "current".to_string()
        } else {
            let mut repaired = current;
            if !repaired.is_empty() && !repaired.ends_with('\n') {
                repaired.push('\n');
            }
            for entry in &missing {
                repaired.push_str(entry);
                repaired.push('\n');
            }
            if let Some(backup) = write_text_atomically(&gitignore_target, repaired, true)? {
                backups.push(backup);
            }
            format!("repaired (added {})", missing.len())
        }
    } else {
        "kept".to_string()
    };
    Ok(InitResult {
        config: config_status,
        gitignore: gitignore_status,
        backups,
    })
}
