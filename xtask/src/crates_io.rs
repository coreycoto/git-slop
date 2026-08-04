use std::collections::BTreeSet;
use std::fs::{self, File};
use std::io::Read;
use std::path::{Component, Path, PathBuf};

use anyhow::{Context, Result, anyhow, bail};
use flate2::read::GzDecoder;
use serde::{Deserialize, Serialize};

use crate::manifest::{
    PROJECT_NAME, is_full_revision, is_sha256, is_strict_semver, resolve_project_path, sha256_file,
};

pub const CRATE_SOURCE_SCHEMA_VERSION: u32 = 1;
pub const CRATES_IO_REGISTRY: &str = "crates.io";

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CrateSource {
    pub schema_version: u32,
    pub registry: String,
    pub package: String,
    pub version: String,
    pub url: String,
    pub sha256: String,
    pub revision: String,
    pub vcs_dirty: bool,
}

impl CrateSource {
    pub fn new(
        version: impl Into<String>,
        sha256: impl Into<String>,
        revision: impl Into<String>,
    ) -> Self {
        let version = version.into();
        Self {
            schema_version: CRATE_SOURCE_SCHEMA_VERSION,
            registry: CRATES_IO_REGISTRY.to_owned(),
            package: PROJECT_NAME.to_owned(),
            url: crate_download_url(&version),
            version,
            sha256: sha256.into(),
            revision: revision.into(),
            vcs_dirty: false,
        }
    }

    pub fn validate(&self) -> Result<()> {
        if self.schema_version != CRATE_SOURCE_SCHEMA_VERSION {
            bail!("crate source schema_version must be {CRATE_SOURCE_SCHEMA_VERSION}");
        }
        if self.registry != CRATES_IO_REGISTRY {
            bail!("crate source registry must be {CRATES_IO_REGISTRY}");
        }
        if self.package != PROJECT_NAME {
            bail!("crate source package must be {PROJECT_NAME}");
        }
        if !is_strict_semver(&self.version) {
            bail!("crate source version must be strict semver");
        }
        let expected_url = crate_download_url(&self.version);
        if self.url != expected_url {
            bail!("crate source url must be {expected_url}");
        }
        if !is_sha256(&self.sha256) {
            bail!("crate source sha256 must be 64 lowercase hexadecimal characters");
        }
        if !is_full_revision(&self.revision) {
            bail!("crate source revision must be a full commit id");
        }
        if self.vcs_dirty {
            bail!("crate source VCS metadata must record a clean worktree");
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VerifyCrateOptions {
    pub project_root: PathBuf,
    pub crate_file: PathBuf,
    pub version: String,
    pub revision: String,
    pub expected_sha256: String,
    pub output: PathBuf,
}

pub fn crate_download_url(version: &str) -> String {
    format!("https://static.crates.io/crates/{PROJECT_NAME}/{PROJECT_NAME}-{version}.crate")
}

pub fn load_crate_source(project_root: &Path, path: &Path) -> Result<CrateSource> {
    let path = resolve_project_path(project_root, path)?;
    let payload = fs::read(&path).with_context(|| format!("unable to read {}", path.display()))?;
    let source = serde_json::from_slice::<CrateSource>(&payload)
        .with_context(|| format!("unable to parse {}", path.display()))?;
    source.validate()?;
    Ok(source)
}

pub fn render_crate_source(source: &CrateSource) -> Result<String> {
    source.validate()?;
    let value = serde_json::to_value(source).context("unable to serialize crate source")?;
    let mut rendered =
        serde_json::to_string_pretty(&value).context("unable to render crate source JSON")?;
    rendered.push('\n');
    Ok(rendered)
}

pub fn verify_crate(options: &VerifyCrateOptions) -> Result<CrateSource> {
    if !is_strict_semver(&options.version) {
        bail!("expected crate version must be strict semver");
    }
    if !is_full_revision(&options.revision) {
        bail!("expected crate revision must be a full commit id");
    }
    if !is_sha256(&options.expected_sha256) {
        bail!("expected crate sha256 must be 64 lowercase hexadecimal characters");
    }

    let crate_file = resolve_project_path(&options.project_root, &options.crate_file)?;
    let actual_sha256 = sha256_file(&crate_file)?;
    if actual_sha256 != options.expected_sha256 {
        bail!(
            "crate SHA-256 mismatch: expected {}, received {actual_sha256}",
            options.expected_sha256
        );
    }

    verify_archive_identity(&crate_file, &options.version, &options.revision)?;
    let source = CrateSource::new(&options.version, actual_sha256, options.revision.clone());
    source.validate()?;

    let output = resolve_project_path(&options.project_root, &options.output)?;
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("unable to create {}", parent.display()))?;
    }
    fs::write(&output, render_crate_source(&source)?)
        .with_context(|| format!("unable to write {}", output.display()))?;
    Ok(source)
}

fn verify_archive_identity(crate_file: &Path, version: &str, revision: &str) -> Result<()> {
    let file = File::open(crate_file)
        .with_context(|| format!("unable to open {}", crate_file.display()))?;
    let decoder = GzDecoder::new(file);
    let mut archive = tar::Archive::new(decoder);
    let prefix = format!("{PROJECT_NAME}-{version}");
    let cargo_path = format!("{prefix}/Cargo.toml");
    let vcs_path = format!("{prefix}/.cargo_vcs_info.json");
    let required = [
        cargo_path.clone(),
        vcs_path.clone(),
        format!("{prefix}/Cargo.lock"),
        format!("{prefix}/LICENSE"),
        format!("{prefix}/README.md"),
        format!("{prefix}/man/git-slop.1"),
        format!("{prefix}/src/main.rs"),
    ]
    .into_iter()
    .collect::<BTreeSet<_>>();
    let mut seen = BTreeSet::new();
    let mut cargo_toml = None;
    let mut vcs_info = None;
    let mut entry_count = 0_usize;
    let mut expanded_size = 0_u64;

    for entry in archive
        .entries()
        .context("unable to enumerate crate archive")?
    {
        let mut entry = entry.context("unable to inspect crate archive entry")?;
        entry_count += 1;
        if entry_count > 4096 {
            bail!("crate archive contains more than 4096 entries");
        }
        let entry_type = entry.header().entry_type();
        if !entry_type.is_file() && !entry_type.is_dir() {
            bail!("crate archive contains a non-file, non-directory entry");
        }
        expanded_size = expanded_size
            .checked_add(
                entry
                    .header()
                    .size()
                    .context("invalid crate archive entry size")?,
            )
            .ok_or_else(|| anyhow!("crate archive expanded size overflow"))?;
        if expanded_size > 64 * 1024 * 1024 {
            bail!("crate archive expands beyond 64 MiB");
        }
        let path = entry
            .path()
            .context("crate archive contains an invalid path")?;
        let path_text = path
            .to_str()
            .ok_or_else(|| anyhow!("crate archive contains a non-UTF-8 path"))?;
        if path_text.contains('\\') {
            bail!("crate archive path must not contain a backslash: {path_text}");
        }
        validate_archive_path(&path, &prefix)?;
        let path = path_text.to_owned();
        if !seen.insert(path.clone()) {
            bail!("crate archive contains duplicate entry {path}");
        }
        if path == cargo_path {
            cargo_toml = Some(read_bounded(&mut entry, &path)?);
        } else if path == vcs_path {
            vcs_info = Some(read_bounded(&mut entry, &path)?);
        }
    }

    let missing = required.difference(&seen).cloned().collect::<Vec<_>>();
    if !missing.is_empty() {
        bail!(
            "crate archive is missing required member(s): {}",
            missing.join(", ")
        );
    }
    verify_cargo_manifest(
        cargo_toml
            .as_deref()
            .ok_or_else(|| anyhow!("missing Cargo.toml"))?,
        version,
    )?;
    verify_vcs_info(
        vcs_info
            .as_deref()
            .ok_or_else(|| anyhow!("missing .cargo_vcs_info.json"))?,
        revision,
    )
}

fn validate_archive_path(path: &Path, prefix: &str) -> Result<()> {
    let components = path.components().collect::<Vec<_>>();
    if components.is_empty()
        || components
            .iter()
            .any(|component| !matches!(component, Component::Normal(_)))
        || components[0].as_os_str() != prefix
    {
        bail!(
            "crate archive contains unsafe or unexpected path {}",
            path.display()
        );
    }
    Ok(())
}

fn read_bounded(reader: &mut impl Read, label: &str) -> Result<Vec<u8>> {
    const LIMIT: u64 = 1024 * 1024;
    let mut payload = Vec::new();
    reader
        .take(LIMIT + 1)
        .read_to_end(&mut payload)
        .with_context(|| format!("unable to read {label}"))?;
    if payload.len() as u64 > LIMIT {
        bail!("crate archive member {label} exceeds {LIMIT} bytes");
    }
    Ok(payload)
}

fn verify_cargo_manifest(payload: &[u8], version: &str) -> Result<()> {
    let text = std::str::from_utf8(payload).context("packaged Cargo.toml is not UTF-8")?;
    let manifest: toml::Value =
        toml::from_str(text).context("unable to parse packaged Cargo.toml")?;
    let package = manifest
        .get("package")
        .and_then(toml::Value::as_table)
        .ok_or_else(|| anyhow!("packaged Cargo.toml must define [package]"))?;
    if package.get("name").and_then(toml::Value::as_str) != Some(PROJECT_NAME) {
        bail!("packaged Cargo.toml package name must be {PROJECT_NAME}");
    }
    if package.get("version").and_then(toml::Value::as_str) != Some(version) {
        bail!("packaged Cargo.toml version must be {version}");
    }
    Ok(())
}

fn verify_vcs_info(payload: &[u8], revision: &str) -> Result<()> {
    let value: serde_json::Value =
        serde_json::from_slice(payload).context("unable to parse .cargo_vcs_info.json")?;
    let git = value
        .get("git")
        .and_then(serde_json::Value::as_object)
        .ok_or_else(|| anyhow!(".cargo_vcs_info.json must define git metadata"))?;
    if git.get("sha1").and_then(serde_json::Value::as_str) != Some(revision) {
        bail!("crate VCS revision does not match expected revision {revision}");
    }
    let dirty = match git.get("dirty") {
        None => false,
        Some(value) => value
            .as_bool()
            .ok_or_else(|| anyhow!("crate VCS metadata git.dirty must be boolean when present"))?,
    };
    if dirty {
        bail!("crate VCS metadata must record dirty=false");
    }
    if value.get("path_in_vcs").and_then(serde_json::Value::as_str) != Some("") {
        bail!("crate VCS metadata path_in_vcs must be empty");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use flate2::Compression;
    use flate2::write::GzEncoder;
    use tar::{Builder, EntryType, Header};

    use super::*;

    const VERSION: &str = "0.9.0";
    const REVISION: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

    fn append_file(
        builder: &mut Builder<GzEncoder<File>>,
        path: &str,
        payload: &[u8],
    ) -> Result<()> {
        let mut header = Header::new_gnu();
        header.set_size(payload.len() as u64);
        header.set_mode(0o644);
        header.set_cksum();
        builder.append_data(&mut header, path, payload)?;
        Ok(())
    }

    fn write_fixture(
        path: &Path,
        revision: &str,
        dirty: Option<bool>,
        special: Option<EntryType>,
    ) -> Result<()> {
        let file = File::create(path)?;
        let encoder = GzEncoder::new(file, Compression::default());
        let mut builder = Builder::new(encoder);
        let prefix = format!("git-slop-{VERSION}");
        append_file(
            &mut builder,
            &format!("{prefix}/Cargo.toml"),
            b"[package]\nname = \"git-slop\"\nversion = \"0.9.0\"\n",
        )?;
        append_file(&mut builder, &format!("{prefix}/Cargo.lock"), b"# lock\n")?;
        append_file(&mut builder, &format!("{prefix}/LICENSE"), b"MIT\n")?;
        append_file(
            &mut builder,
            &format!("{prefix}/README.md"),
            b"# Git Slop\n",
        )?;
        append_file(
            &mut builder,
            &format!("{prefix}/man/git-slop.1"),
            b".TH GIT-SLOP 1\n",
        )?;
        append_file(
            &mut builder,
            &format!("{prefix}/src/main.rs"),
            b"fn main() {}\n",
        )?;
        let mut git = serde_json::Map::new();
        git.insert("sha1".into(), serde_json::Value::String(revision.into()));
        if let Some(dirty) = dirty {
            git.insert("dirty".into(), serde_json::Value::Bool(dirty));
        }
        let vcs = serde_json::json!({ "git": git, "path_in_vcs": "" });
        append_file(
            &mut builder,
            &format!("{prefix}/.cargo_vcs_info.json"),
            serde_json::to_string(&vcs)?.as_bytes(),
        )?;
        if let Some(entry_type) = special {
            let mut header = Header::new_gnu();
            header.set_entry_type(entry_type);
            header.set_size(0);
            header.set_mode(0o777);
            header.set_link_name("../../outside")?;
            header.set_cksum();
            builder.append_data(
                &mut header,
                format!("{prefix}/unsafe-link"),
                std::io::empty(),
            )?;
        }
        let mut encoder = builder.into_inner()?;
        encoder.flush()?;
        encoder.finish()?;
        Ok(())
    }

    #[test]
    fn verifies_exact_registry_crate_and_writes_source_metadata() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let crate_file = temp.path().join("git-slop-0.9.0.crate");
        write_fixture(&crate_file, REVISION, None, None)?;
        let digest = sha256_file(&crate_file)?;
        let options = VerifyCrateOptions {
            project_root: temp.path().to_path_buf(),
            crate_file: PathBuf::from("git-slop-0.9.0.crate"),
            version: VERSION.into(),
            revision: REVISION.into(),
            expected_sha256: digest.clone(),
            output: PathBuf::from("dist/crate-source.json"),
        };
        let source = verify_crate(&options)?;
        assert_eq!(source, CrateSource::new(VERSION, digest, REVISION));
        assert_eq!(
            load_crate_source(temp.path(), Path::new("dist/crate-source.json"))?,
            source
        );
        Ok(())
    }

    #[test]
    fn rejects_digest_revision_and_dirty_drift() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let crate_file = temp.path().join("fixture.crate");
        write_fixture(&crate_file, REVISION, None, None)?;
        let error = verify_archive_identity(&crate_file, VERSION, &"b".repeat(40)).unwrap_err();
        assert!(error.to_string().contains("VCS revision"));

        write_fixture(&crate_file, REVISION, Some(true), None)?;
        let error = verify_archive_identity(&crate_file, VERSION, REVISION).unwrap_err();
        assert!(error.to_string().contains("dirty=false"));

        let bad = CrateSource::new(VERSION, "f".repeat(64), REVISION);
        assert!(bad.validate().is_ok());
        let mut bad = bad;
        bad.sha256 = "F".repeat(64);
        assert!(
            bad.validate()
                .unwrap_err()
                .to_string()
                .contains("lowercase")
        );
        Ok(())
    }

    #[test]
    fn rejects_symlink_and_hardlink_entries() -> Result<()> {
        let temp = tempfile::tempdir()?;
        for (name, entry_type) in [
            ("symlink.crate", EntryType::Symlink),
            ("hardlink.crate", EntryType::Link),
        ] {
            let path = temp.path().join(name);
            write_fixture(&path, REVISION, None, Some(entry_type))?;
            let error = verify_archive_identity(&path, VERSION, REVISION).unwrap_err();
            assert!(
                error.to_string().contains("non-file, non-directory entry"),
                "{error:#}"
            );
        }
        Ok(())
    }

    #[test]
    fn rejects_entries_outside_the_single_package_prefix() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let path = temp.path().join("wrong-prefix.crate");
        let file = File::create(&path)?;
        let encoder = GzEncoder::new(file, Compression::default());
        let mut builder = Builder::new(encoder);
        append_file(&mut builder, "another-package/Cargo.toml", b"invalid")?;
        builder.into_inner()?.finish()?;
        let error = verify_archive_identity(&path, VERSION, REVISION).unwrap_err();
        assert!(error.to_string().contains("unsafe or unexpected path"));
        Ok(())
    }

    #[test]
    fn rejects_raw_backslashes_in_archive_paths() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let path = temp.path().join("backslash.crate");
        let file = File::create(&path)?;
        let encoder = GzEncoder::new(file, Compression::default());
        let mut builder = Builder::new(encoder);
        append_file(
            &mut builder,
            "git-slop-0.9.0/src\\unexpected.rs",
            b"fn main() {}\n",
        )?;
        builder.into_inner()?.finish()?;
        let error = verify_archive_identity(&path, VERSION, REVISION).unwrap_err();
        assert!(error.to_string().contains("must not contain a backslash"));
        Ok(())
    }
}
