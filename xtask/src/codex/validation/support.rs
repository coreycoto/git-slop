fn validate_normalized_contract(
    repo_root: &Path,
    relative: &str,
    required: &[&str],
    forbidden: &[&str],
    errors: &mut Vec<String>,
) {
    let Some(text) = read_text(repo_root, relative, errors) else {
        return;
    };
    let normalized = text.split_whitespace().collect::<Vec<_>>().join(" ");
    for expected in required {
        if !normalized.contains(expected) {
            errors.push(format!("{relative} must include {expected}."));
        }
    }
    for unexpected in forbidden {
        if normalized.contains(unexpected) {
            errors.push(format!("{relative} must not include {unexpected}."));
        }
    }
}

fn load_toml(repo_root: &Path, relative: &str, errors: &mut Vec<String>) -> Option<TomlValue> {
    let text = read_text(repo_root, relative, errors)?;
    match toml::from_str(&text) {
        Ok(value) => Some(value),
        Err(error) => {
            errors.push(format!("Unable to parse {relative}: {error}"));
            None
        }
    }
}

fn load_json(repo_root: &Path, relative: &str, errors: &mut Vec<String>) -> Option<JsonValue> {
    let text = read_text(repo_root, relative, errors)?;
    match serde_json::from_str(&text) {
        Ok(value) => Some(value),
        Err(error) => {
            errors.push(format!("Unable to parse {relative}: {error}"));
            None
        }
    }
}

fn read_text(repo_root: &Path, relative: &str, errors: &mut Vec<String>) -> Option<String> {
    match fs::read_to_string(repo_root.join(relative)) {
        Ok(text) => Some(text),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            errors.push(format!("{relative} is missing."));
            None
        }
        Err(error) => {
            errors.push(format!("Unable to read {relative}: {error}"));
            None
        }
    }
}

fn toml_string<'a>(value: &'a TomlValue, key: &str) -> Option<&'a str> {
    value.get(key).and_then(TomlValue::as_str)
}

fn json_string<'a>(value: &'a JsonValue, key: &str) -> Option<&'a str> {
    value.get(key).and_then(JsonValue::as_str)
}

fn command_on_path(command: &str) -> bool {
    let Some(path) = env::var_os("PATH") else {
        return false;
    };
    env::split_paths(&path)
        .any(|directory| command_candidates(&directory, command).any(|path| path.is_file()))
}

#[cfg(not(windows))]
fn command_candidates(directory: &Path, command: &str) -> impl Iterator<Item = PathBuf> {
    [directory.join(command)].into_iter()
}

#[cfg(windows)]
fn command_candidates(directory: &Path, command: &str) -> impl Iterator<Item = PathBuf> {
    [
        directory.join(command),
        directory.join(format!("{command}.exe")),
        directory.join(format!("{command}.cmd")),
        directory.join(format!("{command}.bat")),
    ]
    .into_iter()
}
