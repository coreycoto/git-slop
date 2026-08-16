pub fn is_strict_semver(version: &str) -> bool {
    let mut parts = version.split('.');
    let valid_part = |part: &str| {
        !part.is_empty()
            && part.bytes().all(|byte| byte.is_ascii_digit())
            && (part == "0" || !part.starts_with('0'))
    };
    parts.by_ref().take(3).all(valid_part)
        && version.matches('.').count() == 2
        && parts.next().is_none()
}

pub fn is_full_revision(revision: &str) -> bool {
    revision.len() == 40
        && revision
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
}

pub fn is_sha256(digest: &str) -> bool {
    digest.len() == 64
        && digest
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
}

pub fn strict_tag_version(tag: &str) -> Result<&str> {
    let Some(version) = tag.strip_prefix('v') else {
        bail!("release tag must be strict semver in vX.Y.Z form: {tag}");
    };
    if !is_strict_semver(version) {
        bail!("release tag must be strict semver in vX.Y.Z form: {tag}");
    }
    Ok(version)
}

pub fn project_version(project_root: &Path) -> Result<String> {
    let cargo_path = project_root.join("Cargo.toml");
    let text = fs::read_to_string(&cargo_path)
        .with_context(|| format!("unable to read {}", cargo_path.display()))?;
    let payload: toml::Value = toml::from_str(&text)
        .with_context(|| format!("unable to parse {}", cargo_path.display()))?;
    payload
        .get("package")
        .and_then(|package| package.get("version"))
        .and_then(toml::Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| anyhow!("Cargo.toml must define package.version"))
}

pub fn git_revision(project_root: &Path, release_tag: &str) -> Result<String> {
    git_revision_with_runner(project_root, release_tag, &mut SystemCommandRunner)
}

pub fn git_revision_with_runner(
    project_root: &Path,
    release_tag: &str,
    runner: &mut impl CommandRunner,
) -> Result<String> {
    let command = CommandSpec::new(
        "git",
        [
            "rev-parse".to_owned(),
            "--verify".to_owned(),
            format!("refs/tags/{release_tag}^{{commit}}"),
        ],
    );
    runner.output(project_root, &command)
}

pub fn artifact_name(tag: &str, target: ReleaseTarget) -> String {
    format!("git-slop-{tag}-{}.{}", target.target, target.archive)
}
