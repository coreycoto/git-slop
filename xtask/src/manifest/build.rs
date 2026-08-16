pub fn build_manifest(
    project_root: &Path,
    dist_dir: &Path,
    crate_source: &CrateSource,
    tag: Option<&str>,
) -> Result<ReleaseManifest> {
    build_manifest_with_runner(
        project_root,
        dist_dir,
        crate_source,
        tag,
        &mut SystemCommandRunner,
    )
}

pub fn build_manifest_with_runner(
    project_root: &Path,
    dist_dir: &Path,
    crate_source: &CrateSource,
    tag: Option<&str>,
    runner: &mut impl CommandRunner,
) -> Result<ReleaseManifest> {
    let version = project_version(project_root)?;
    let release_tag = tag.map_or_else(|| format!("v{version}"), str::to_owned);
    let tag_version = strict_tag_version(&release_tag)?;
    if tag_version != version {
        bail!("Cargo.toml version is {version}; release tag {release_tag} is {tag_version}.");
    }
    crate_source.validate()?;
    if crate_source.version != version {
        bail!(
            "Cargo.toml version is {version}; canonical crate version is {}.",
            crate_source.version
        );
    }
    if !dist_dir.is_dir() {
        bail!("dist dir does not exist: {}", dist_dir.display());
    }

    let artifacts = release_artifacts(dist_dir, &release_tag)?;
    let revision = git_revision_with_runner(project_root, &release_tag, runner)?;
    if crate_source.revision != revision {
        bail!(
            "release tag {release_tag} resolves to {revision}, but canonical crate revision is {}.",
            crate_source.revision
        );
    }
    let release_url =
        format!("https://github.com/{REPO_FULL_NAME}/releases/download/{release_tag}");
    let mut manifest = ReleaseManifest::new(
        ReleaseIdentity::new(version, &release_tag, revision, crate_source.clone()),
        artifacts,
        ChecksumMetadata {
            algorithm: "sha256".to_owned(),
            name: CHECKSUM_FILE_NAME.to_owned(),
            url: format!("{release_url}/{CHECKSUM_FILE_NAME}"),
        },
        install_instructions(&release_tag),
    );
    manifest.supplemental_assets = supplemental_release_assets(dist_dir, &release_url)?;
    manifest.validate()?;
    Ok(manifest)
}

fn supplemental_release_assets(
    dist_dir: &Path,
    release_url: &str,
) -> Result<Vec<SupplementalReleaseAsset>> {
    let present = SUPPLEMENTAL_RELEASE_ASSETS
        .iter()
        .filter(|(name, _, _)| dist_dir.join(name).is_file())
        .count();
    if present == 0 {
        return Ok(Vec::new());
    }
    if present != SUPPLEMENTAL_RELEASE_ASSETS.len() {
        bail!("supplemental release assets must be generated as one complete set");
    }
    SUPPLEMENTAL_RELEASE_ASSETS
        .into_iter()
        .map(|(name, role, media_type)| {
            let path = dist_dir.join(name);
            let size_bytes = fs::metadata(&path)
                .with_context(|| format!("unable to inspect {}", path.display()))?
                .len();
            if size_bytes == 0 {
                bail!("supplemental release asset {name} must not be empty");
            }
            Ok(SupplementalReleaseAsset {
                name: name.to_owned(),
                path: name.to_owned(),
                role: role.to_owned(),
                media_type: media_type.to_owned(),
                required: true,
                contract_version: 1,
                sha256: sha256_file(&path)?,
                size_bytes,
                url: format!("{release_url}/{name}"),
            })
        })
        .collect()
}

fn release_artifacts(dist_dir: &Path, tag: &str) -> Result<Vec<ReleaseArtifact>> {
    let release_url = format!("https://github.com/{REPO_FULL_NAME}/releases/download/{tag}");
    let mut artifacts = Vec::with_capacity(RELEASE_TARGETS.len());
    let mut missing = Vec::new();

    for target in RELEASE_TARGETS {
        let name = artifact_name(tag, target);
        let path = dist_dir.join(&name);
        if !path.is_file() {
            missing.push(name);
            continue;
        }
        let metadata =
            fs::metadata(&path).with_context(|| format!("unable to inspect {}", path.display()))?;
        if metadata.len() == 0 || metadata.len() > MAX_RELEASE_ARTIFACT_BYTES {
            bail!(
                "release artifact {name} size must be from 1 through \
                 {MAX_RELEASE_ARTIFACT_BYTES} bytes"
            );
        }
        artifacts.push(ReleaseArtifact {
            name: name.clone(),
            path: name.clone(),
            target: target.target.to_owned(),
            os: target.os.to_owned(),
            arch: target.arch.to_owned(),
            archive: target.archive.to_owned(),
            sha256: sha256_file(&path)?,
            size_bytes: metadata.len(),
            url: format!("{release_url}/{name}"),
        });
    }

    if !missing.is_empty() {
        bail!(
            "missing required release artifact(s): {}",
            missing.join(", ")
        );
    }

    let expected_names = artifacts
        .iter()
        .map(|artifact| artifact.name.as_str())
        .collect::<BTreeSet<_>>();
    let prefix = format!("git-slop-{tag}-");
    let mut unexpected = Vec::new();
    for entry in
        fs::read_dir(dist_dir).with_context(|| format!("unable to list {}", dist_dir.display()))?
    {
        let entry = entry.with_context(|| format!("unable to inspect {}", dist_dir.display()))?;
        if !entry.path().is_file() {
            continue;
        }
        let name = entry.file_name().into_string().map_err(|name| {
            anyhow!(
                "release distribution contains a non-UTF-8 filename: {:?}",
                name
            )
        })?;
        if name.starts_with(&prefix) && !expected_names.contains(name.as_str()) {
            unexpected.push(name);
        }
    }
    unexpected.sort();
    if !unexpected.is_empty() {
        bail!("unexpected release artifact(s): {}", unexpected.join(", "));
    }

    Ok(artifacts)
}
