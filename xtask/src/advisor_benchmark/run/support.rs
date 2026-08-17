pub(super) fn privacy_safe_benchmark_runtime_identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 200
        && value
            .bytes()
            .next()
            .is_some_and(|byte| byte.is_ascii_alphanumeric())
        && !value.ends_with(':')
        && !value.contains(":/")
        && !value.contains("::")
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"._+:/@-".contains(&byte))
}

fn prepare_review_directory(options: &Options) -> Result<Option<PathBuf>> {
    let Some(requested) = options.review_output_dir.as_deref() else {
        return Ok(None);
    };
    if options.prepare_only {
        bail!("--review-output-dir is available only for inference runs");
    }
    if !requested.is_absolute() {
        bail!("--review-output-dir must be an absolute private path outside the repository");
    }
    if requested
        .symlink_metadata()
        .is_ok_and(|metadata| metadata.file_type().is_symlink())
    {
        bail!("--review-output-dir must not be a symbolic link");
    }
    let resolved = if requested.exists() {
        fs::canonicalize(requested)?
    } else {
        let parent = requested.parent().ok_or_else(|| {
            anyhow::anyhow!("--review-output-dir must have an existing parent directory")
        })?;
        fs::canonicalize(parent)?.join(
            requested
                .file_name()
                .ok_or_else(|| anyhow::anyhow!("invalid --review-output-dir"))?,
        )
    };
    if resolved.starts_with(fs::canonicalize(&options.repo_root)?) {
        bail!("--review-output-dir must remain outside the public repository");
    }
    if resolved.exists() {
        if !resolved.is_dir() || fs::read_dir(&resolved)?.next().is_some() {
            bail!("--review-output-dir must be a new or empty directory");
        }
    } else {
        fs::create_dir(&resolved)?;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&resolved, fs::Permissions::from_mode(0o700))?;
    }
    eprintln!(
        "Writing explicitly requested private review artifacts outside the repository; do not commit or share their contents."
    );
    Ok(Some(resolved))
}

pub(super) fn write_review_artifact(directory: &Path, name: &str, bytes: &[u8]) -> Result<()> {
    let path = directory.join(name);
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(&path)?;
    use std::io::Write;
    file.write_all(bytes)?;
    file.sync_all()?;
    sync_benchmark_directory(directory)?;
    Ok(())
}

fn write_terminal_outputs(
    options: &Options,
    inputs: &OutputInputs<'_>,
    started: u128,
    samples: &[Sample],
    termination_reason: Option<&str>,
    review_directory: Option<&Path>,
    review_entries: &[ReviewManifestEntry],
) -> Result<(PathBuf, PathBuf)> {
    let paths = write_outputs(options, inputs, started, samples, None, termination_reason)?;
    if let Some(directory) = review_directory {
        write_review_manifests(
            directory,
            review_entries,
            &paths.0,
            termination_reason.is_none(),
        )?;
    }
    Ok(paths)
}
