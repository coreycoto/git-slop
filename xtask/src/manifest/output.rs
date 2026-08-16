pub fn sha256_file(path: &Path) -> Result<String> {
    let file = File::open(path).with_context(|| format!("unable to read {}", path.display()))?;
    let mut reader = BufReader::new(file);
    let mut digest = Sha256::new();
    let mut buffer = [0_u8; 1024 * 1024];
    loop {
        let count = reader
            .read(&mut buffer)
            .with_context(|| format!("unable to read {}", path.display()))?;
        if count == 0 {
            break;
        }
        digest.update(&buffer[..count]);
    }
    Ok(hex::encode(digest.finalize()))
}

pub fn checksum_lines(artifacts: &[ReleaseArtifact]) -> String {
    let mut lines = artifacts
        .iter()
        .map(|artifact| format!("{}  {}\n", artifact.sha256, artifact.name))
        .collect::<Vec<_>>();
    lines.sort();
    lines.concat()
}

pub fn checksum_lines_with_manifest(
    artifacts: &[ReleaseArtifact],
    supplemental_assets: &[SupplementalReleaseAsset],
    manifest_name: &str,
    manifest_sha256: &str,
) -> String {
    let mut lines = artifacts
        .iter()
        .map(|artifact| (artifact.name.as_str(), artifact.sha256.as_str()))
        .chain(
            supplemental_assets
                .iter()
                .map(|artifact| (artifact.name.as_str(), artifact.sha256.as_str())),
        )
        .chain(std::iter::once((manifest_name, manifest_sha256)))
        .map(|(name, digest)| format!("{digest}  {name}\n"))
        .collect::<Vec<_>>();
    lines.sort();
    lines.concat()
}

pub fn render_manifest_json(manifest: &ReleaseManifest) -> Result<String> {
    manifest.validate()?;
    // serde_json::Value uses a sorted map without preserve_order, matching the
    // previous recursive sort_keys=True output rather than Rust field order.
    let value = serde_json::to_value(manifest).context("unable to serialize release manifest")?;
    let mut rendered =
        serde_json::to_string_pretty(&value).context("unable to render release manifest JSON")?;
    rendered.push('\n');
    Ok(rendered)
}

pub fn write_manifest_outputs(
    project_root: &Path,
    dist_dir: &Path,
    manifest: &ReleaseManifest,
    output: &Path,
    checksum_output: &Path,
) -> Result<ManifestOutputPaths> {
    let dist_dir = resolve_project_path(project_root, dist_dir)?;
    let output = resolve_project_path(project_root, output)?;
    let checksum_output = resolve_project_path(project_root, checksum_output)?;
    if output == checksum_output {
        bail!(
            "release manifest and checksum outputs must be different paths: {}",
            output.display()
        );
    }
    for artifact in &manifest.artifacts {
        let source = resolve_project_path(project_root, &dist_dir.join(&artifact.path))?;
        if output == source || checksum_output == source {
            bail!(
                "release metadata output must not overwrite source archive {}",
                source.display()
            );
        }
    }
    create_parent(&output)?;
    create_parent(&checksum_output)?;
    fs::write(&output, render_manifest_json(manifest)?)
        .with_context(|| format!("unable to write {}", output.display()))?;
    let manifest_name = output
        .file_name()
        .and_then(OsStr::to_str)
        .ok_or_else(|| anyhow!("release manifest output must have a UTF-8 filename"))?;
    let manifest_sha256 = sha256_file(&output)?;
    fs::write(
        &checksum_output,
        checksum_lines_with_manifest(
            &manifest.artifacts,
            &manifest.supplemental_assets,
            manifest_name,
            &manifest_sha256,
        ),
    )
    .with_context(|| format!("unable to write {}", checksum_output.display()))?;
    Ok(ManifestOutputPaths {
        manifest: output,
        checksums: checksum_output,
    })
}

pub fn resolve_project_path(project_root: &Path, path: &Path) -> Result<PathBuf> {
    let joined = if path.is_absolute() {
        path.to_path_buf()
    } else {
        project_root.join(path)
    };
    lexical_absolute(&joined)
}

fn lexical_absolute(path: &Path) -> Result<PathBuf> {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .context("unable to resolve current directory")?
            .join(path)
    };
    let mut normalized = PathBuf::new();
    for component in absolute.components() {
        match component {
            Component::Prefix(prefix) => normalized.push(prefix.as_os_str()),
            Component::RootDir => normalized.push(OsStr::new(std::path::MAIN_SEPARATOR_STR)),
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            Component::Normal(part) => normalized.push(part),
        }
    }
    Ok(normalized)
}

fn create_parent(path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("unable to create {}", parent.display()))?;
    }
    Ok(())
}
