#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReleaseTarget {
    pub target: &'static str,
    pub os: &'static str,
    pub arch: &'static str,
    pub archive: &'static str,
}

/// The complete supported release matrix, kept in deterministic target order.
pub const RELEASE_TARGETS: [ReleaseTarget; 7] = [
    ReleaseTarget {
        target: "aarch64-apple-darwin",
        os: "macos",
        arch: "aarch64",
        archive: "tar.gz",
    },
    ReleaseTarget {
        target: "aarch64-pc-windows-msvc",
        os: "windows",
        arch: "aarch64",
        archive: "zip",
    },
    ReleaseTarget {
        target: "aarch64-unknown-linux-gnu",
        os: "linux",
        arch: "aarch64",
        archive: "tar.gz",
    },
    ReleaseTarget {
        target: "x86_64-apple-darwin",
        os: "macos",
        arch: "x86_64",
        archive: "tar.gz",
    },
    ReleaseTarget {
        target: "x86_64-pc-windows-msvc",
        os: "windows",
        arch: "x86_64",
        archive: "zip",
    },
    ReleaseTarget {
        target: "x86_64-unknown-linux-gnu",
        os: "linux",
        arch: "x86_64",
        archive: "tar.gz",
    },
    ReleaseTarget {
        target: "x86_64-unknown-linux-musl",
        os: "linux",
        arch: "x86_64",
        archive: "tar.gz",
    },
];

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommandSpec {
    pub program: String,
    pub args: Vec<String>,
}

impl CommandSpec {
    pub fn new<I, S>(program: impl Into<String>, args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self {
            program: program.into(),
            args: args.into_iter().map(Into::into).collect(),
        }
    }

    pub fn display(&self) -> String {
        std::iter::once(self.program.as_str())
            .chain(self.args.iter().map(String::as_str))
            .collect::<Vec<_>>()
            .join(" ")
    }
}

/// Injectable command boundary used by release orchestration and exact Git lookups.
pub trait CommandRunner {
    fn output(&mut self, cwd: &Path, command: &CommandSpec) -> Result<String>;
    fn run(&mut self, cwd: &Path, command: &CommandSpec) -> Result<()>;
}

#[derive(Clone, Copy, Debug, Default)]
pub struct SystemCommandRunner;

impl CommandRunner for SystemCommandRunner {
    fn output(&mut self, cwd: &Path, command: &CommandSpec) -> Result<String> {
        let output = Command::new(&command.program)
            .args(&command.args)
            .current_dir(cwd)
            .output()
            .with_context(|| format!("unable to run {}", command.display()))?;
        if !output.status.success() {
            let detail = command_failure_detail(&output.stdout, &output.stderr);
            bail!("{} failed{}", command.display(), detail);
        }
        String::from_utf8(output.stdout)
            .with_context(|| format!("{} produced non-UTF-8 output", command.display()))
            .map(|output| output.trim().to_owned())
    }

    fn run(&mut self, cwd: &Path, command: &CommandSpec) -> Result<()> {
        let status = Command::new(&command.program)
            .args(&command.args)
            .current_dir(cwd)
            .status()
            .with_context(|| format!("unable to run {}", command.display()))?;
        if !status.success() {
            bail!("{} failed with {status}", command.display());
        }
        Ok(())
    }
}

fn command_failure_detail(stdout: &[u8], stderr: &[u8]) -> String {
    let stdout = String::from_utf8_lossy(stdout);
    let stderr = String::from_utf8_lossy(stderr);
    let detail = format!("{}{}", stdout.trim(), stderr.trim());
    if detail.is_empty() {
        String::new()
    } else {
        format!(": {detail}")
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ReleaseIdentity {
    pub schema_version: u32,
    pub project: String,
    pub version: String,
    pub tag: String,
    pub revision: String,
    pub repository: String,
    pub crate_source: CrateSource,
}

impl ReleaseIdentity {
    pub fn new(
        version: impl Into<String>,
        tag: impl Into<String>,
        revision: impl Into<String>,
        crate_source: CrateSource,
    ) -> Self {
        let version = version.into();
        let tag = tag.into();
        let revision = revision.into();
        Self {
            schema_version: MANIFEST_SCHEMA_VERSION,
            project: PROJECT_NAME.to_owned(),
            version,
            tag,
            revision,
            repository: REPO_FULL_NAME.to_owned(),
            crate_source,
        }
    }

    pub fn validate(&self) -> Result<()> {
        if self.schema_version != MANIFEST_SCHEMA_VERSION {
            bail!("release manifest schema_version must be {MANIFEST_SCHEMA_VERSION}");
        }
        if self.project != PROJECT_NAME {
            bail!("release manifest project must be {PROJECT_NAME}");
        }
        if self.repository != REPO_FULL_NAME {
            bail!("release manifest repository must be {REPO_FULL_NAME}");
        }
        if !is_strict_semver(&self.version) {
            bail!("release manifest version must be strict semver");
        }
        if self.tag != format!("v{}", self.version) {
            bail!("release manifest tag must agree with version");
        }
        if !is_full_revision(&self.revision) {
            bail!("release manifest revision must be a full commit id");
        }
        self.crate_source.validate()?;
        if self.crate_source.version != self.version {
            bail!("crate source version must agree with the release version");
        }
        if self.crate_source.revision != self.revision {
            bail!("crate source revision must agree with the release revision");
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ReleaseArtifact {
    pub name: String,
    pub path: String,
    pub target: String,
    pub os: String,
    pub arch: String,
    pub archive: String,
    pub sha256: String,
    pub size_bytes: u64,
    pub url: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SupplementalReleaseAsset {
    pub name: String,
    pub path: String,
    pub role: String,
    pub media_type: String,
    pub required: bool,
    pub contract_version: u32,
    pub sha256: String,
    pub size_bytes: u64,
    pub url: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ChecksumMetadata {
    pub algorithm: String,
    pub name: String,
    pub url: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct InstallInstructions {
    pub attestation: Vec<String>,
    pub cargo: Vec<String>,
    pub homebrew_tap: Vec<String>,
    pub github_release: Vec<String>,
    pub scoop: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ReleaseManifest {
    pub schema_version: u32,
    pub project: String,
    pub version: String,
    pub tag: String,
    pub revision: String,
    pub repository: String,
    pub crate_source: CrateSource,
    pub artifacts: Vec<ReleaseArtifact>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub supplemental_assets: Vec<SupplementalReleaseAsset>,
    pub checksums: ChecksumMetadata,
    pub install: InstallInstructions,
}

impl ReleaseManifest {
    pub fn new(
        identity: ReleaseIdentity,
        artifacts: Vec<ReleaseArtifact>,
        checksums: ChecksumMetadata,
        install: InstallInstructions,
    ) -> Self {
        Self {
            schema_version: identity.schema_version,
            project: identity.project,
            version: identity.version,
            tag: identity.tag,
            revision: identity.revision,
            repository: identity.repository,
            crate_source: identity.crate_source,
            artifacts,
            supplemental_assets: Vec::new(),
            checksums,
            install,
        }
    }

    pub fn identity(&self) -> ReleaseIdentity {
        ReleaseIdentity {
            schema_version: self.schema_version,
            project: self.project.clone(),
            version: self.version.clone(),
            tag: self.tag.clone(),
            revision: self.revision.clone(),
            repository: self.repository.clone(),
            crate_source: self.crate_source.clone(),
        }
    }

    pub fn validate(&self) -> Result<()> {
        self.identity().validate()?;
        if self.artifacts.len() != RELEASE_TARGETS.len() {
            bail!(
                "release manifest must contain exactly {} artifacts",
                RELEASE_TARGETS.len()
            );
        }
        let unique_names = self
            .artifacts
            .iter()
            .map(|artifact| artifact.name.as_str())
            .collect::<BTreeSet<_>>();
        let unique_paths = self
            .artifacts
            .iter()
            .map(|artifact| artifact.path.as_str())
            .collect::<BTreeSet<_>>();
        let unique_targets = self
            .artifacts
            .iter()
            .map(|artifact| artifact.target.as_str())
            .collect::<BTreeSet<_>>();
        if unique_names.len() != self.artifacts.len()
            || unique_paths.len() != self.artifacts.len()
            || unique_targets.len() != self.artifacts.len()
        {
            bail!("release manifest artifact names, paths, and targets must be unique");
        }
        let release_url = format!(
            "https://github.com/{REPO_FULL_NAME}/releases/download/{}",
            self.tag
        );
        for target in RELEASE_TARGETS {
            let artifact = self
                .artifacts
                .iter()
                .find(|artifact| artifact.target == target.target)
                .ok_or_else(|| anyhow!("release manifest is missing target {}", target.target))?;
            let name = artifact_name(&self.tag, target);
            if artifact.name != name || artifact.path != name {
                bail!(
                    "release artifact {} must use exact name and path {name}",
                    target.target
                );
            }
            if artifact.os != target.os
                || artifact.arch != target.arch
                || artifact.archive != target.archive
            {
                bail!("release artifact {name} platform metadata does not match its target");
            }
            if !is_sha256(&artifact.sha256) {
                bail!("release artifact {name} must have a lowercase SHA-256 digest");
            }
            if artifact.size_bytes == 0 || artifact.size_bytes > MAX_RELEASE_ARTIFACT_BYTES {
                bail!(
                    "release artifact {name} size must be from 1 through \
                     {MAX_RELEASE_ARTIFACT_BYTES} bytes"
                );
            }
            if artifact.url != format!("{release_url}/{name}") {
                bail!("release artifact {name} URL does not match the release identity");
            }
        }
        if !self.supplemental_assets.is_empty() {
            if self.supplemental_assets.len() != SUPPLEMENTAL_RELEASE_ASSETS.len() {
                bail!(
                    "release manifest supplemental asset inventory must contain exactly {} entries",
                    SUPPLEMENTAL_RELEASE_ASSETS.len()
                );
            }
            let mut names = BTreeSet::new();
            for (name, role, media_type) in SUPPLEMENTAL_RELEASE_ASSETS {
                let asset = self
                    .supplemental_assets
                    .iter()
                    .find(|asset| asset.name == name)
                    .ok_or_else(|| {
                        anyhow!("release manifest is missing supplemental asset {name}")
                    })?;
                if !names.insert(asset.name.as_str())
                    || asset.path != name
                    || asset.role != role
                    || asset.media_type != media_type
                    || !asset.required
                    || asset.contract_version != 1
                    || !is_sha256(&asset.sha256)
                    || asset.size_bytes == 0
                    || asset.url != format!("{release_url}/{name}")
                {
                    bail!("release manifest supplemental asset {name} has invalid metadata");
                }
            }
        }
        let expected_checksums = ChecksumMetadata {
            algorithm: "sha256".to_owned(),
            name: CHECKSUM_FILE_NAME.to_owned(),
            url: format!("{release_url}/{CHECKSUM_FILE_NAME}"),
        };
        if self.checksums != expected_checksums {
            bail!("release manifest checksum metadata must match the exact release URL");
        }
        if self.install != install_instructions(&self.tag) {
            bail!("release manifest install metadata must match the canonical commands");
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ManifestOutputPaths {
    pub manifest: PathBuf,
    pub checksums: PathBuf,
}
