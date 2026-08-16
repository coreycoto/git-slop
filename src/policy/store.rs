use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

use super::ResolvedPack;
use super::validate::load_and_validate_pack;

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CacheIndex {
    #[serde(default = "index_schema")]
    schema_version: u64,
    #[serde(default)]
    packs: BTreeMap<String, String>,
}

fn index_schema() -> u64 {
    1
}

fn valid_content_digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
}

pub(crate) fn policy_home() -> Result<PathBuf> {
    if let Some(path) = env::var_os("GIT_SLOP_POLICY_HOME") {
        return Ok(PathBuf::from(path));
    }
    if let Some(path) = env::var_os("XDG_DATA_HOME") {
        return Ok(PathBuf::from(path).join("git-slop/policies"));
    }
    if let Some(path) = env::var_os("LOCALAPPDATA") {
        return Ok(PathBuf::from(path).join("git-slop/policies"));
    }
    if let Some(path) = env::var_os("HOME") {
        return Ok(PathBuf::from(path).join(".local/share/git-slop/policies"));
    }
    bail!("unable to resolve policy cache; set GIT_SLOP_POLICY_HOME")
}

fn index_path(home: &Path) -> PathBuf {
    home.join("index.json")
}

fn read_index(home: &Path) -> Result<CacheIndex> {
    let path = index_path(home);
    if !path.exists() {
        return Ok(CacheIndex {
            schema_version: 1,
            packs: BTreeMap::new(),
        });
    }
    let index: CacheIndex = serde_json::from_slice(&fs::read(&path)?)
        .with_context(|| format!("unable to parse policy cache index {}", path.display()))?;
    if index.schema_version != 1 {
        bail!(
            "unsupported policy cache index schema {}",
            index.schema_version
        );
    }
    for (id, digest) in &index.packs {
        if !valid_content_digest(digest) {
            bail!("policy cache index has an invalid content digest for {id}");
        }
    }
    Ok(index)
}

fn write_index(home: &Path, index: &CacheIndex) -> Result<()> {
    fs::create_dir_all(home)?;
    let path = index_path(home);
    crate::config::write_text_atomically(&path, serde_json::to_string_pretty(index)? + "\n", false)
        .map(|_| ())
        .with_context(|| format!("unable to publish policy cache index {}", path.display()))
}

fn copy_file(source: &Path, destination: &Path) -> Result<()> {
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::copy(source, destination).with_context(|| {
        format!(
            "unable to copy policy-pack file {} to {}",
            source.display(),
            destination.display()
        )
    })?;
    Ok(())
}

pub(super) fn install(pack: &ResolvedPack) -> Result<PathBuf> {
    let home = policy_home()?;
    fs::create_dir_all(&home)?;
    let destination = home.join(&pack.content_digest);
    if destination.exists() {
        if fs::symlink_metadata(&destination)?.file_type().is_symlink() {
            bail!(
                "content-addressed policy cache entry must not be a symlink: {}",
                destination.display()
            );
        }
        let existing = load_and_validate_pack(&destination)?;
        if existing.content_digest != pack.content_digest
            || existing.manifest.id != pack.manifest.id
        {
            bail!(
                "content-addressed policy cache entry is inconsistent: {}",
                destination.display()
            );
        }
    } else {
        let temporary = home.join(format!(
            ".{}-{}-tmp",
            pack.content_digest,
            std::process::id()
        ));
        if temporary.exists() {
            fs::remove_dir_all(&temporary)?;
        }
        fs::create_dir(&temporary)?;
        let declared = std::iter::once("git-slop-policy.yaml".to_string())
            .chain(pack.manifest.entrypoints.iter().cloned())
            .chain(pack.manifest.tests.iter().cloned())
            .chain([
                "README.md".to_string(),
                "LICENSE".to_string(),
                "LICENCE".to_string(),
            ]);
        for relative in declared {
            let source = pack.root.join(&relative);
            if source.is_file() {
                copy_file(&source, &temporary.join(&relative))?;
            }
        }
        fs::rename(&temporary, &destination)?;
    }
    let mut index = read_index(&home)?;
    index
        .packs
        .insert(pack.manifest.id.clone(), pack.content_digest.clone());
    write_index(&home, &index)?;
    Ok(destination)
}

pub(super) fn installed(id: &str) -> Result<ResolvedPack> {
    let home = policy_home()?;
    let index = read_index(&home)?;
    let digest = index
        .packs
        .get(id)
        .ok_or_else(|| anyhow::anyhow!("policy pack is not installed: {id}"))?;
    let mut pack = load_and_validate_pack(&home.join(digest))?;
    if pack.manifest.id != id || pack.content_digest != *digest {
        bail!("installed policy pack {id} no longer matches its cache index digest");
    }
    pack.source_type = "user-cache".to_string();
    pack.source_revision = digest.clone();
    Ok(pack)
}

pub(super) fn all_installed() -> Result<Vec<ResolvedPack>> {
    let home = policy_home()?;
    let index = read_index(&home)?;
    index
        .packs
        .keys()
        .map(|id| installed(id))
        .collect::<Result<Vec<_>>>()
}

pub(super) fn remove(id: &str) -> Result<bool> {
    let home = policy_home()?;
    let mut index = read_index(&home)?;
    let Some(digest) = index.packs.remove(id) else {
        return Ok(false);
    };
    let still_referenced = index.packs.values().any(|candidate| candidate == &digest);
    write_index(&home, &index)?;
    if !still_referenced {
        let path = home.join(&digest);
        if path.exists() && fs::symlink_metadata(&path)?.file_type().is_symlink() {
            bail!(
                "refusing to remove a symlinked policy cache entry: {}",
                path.display()
            );
        }
        if path.is_dir() {
            fs::remove_dir_all(path)?;
        }
    }
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_index_rejects_noncanonical_or_traversing_digests() {
        let home = tempfile::tempdir().expect("policy home");
        for digest in ["../outside".to_string(), "A".repeat(64), "g".repeat(64)] {
            fs::write(
                home.path().join("index.json"),
                serde_json::to_vec(&serde_json::json!({
                    "schema_version": 1,
                    "packs": {"com.example.pack": digest.clone()}
                }))
                .unwrap(),
            )
            .unwrap();
            assert!(read_index(home.path()).is_err(), "accepted {digest}");
        }
        fs::write(
            home.path().join("index.json"),
            serde_json::to_vec(&serde_json::json!({
                "schema_version": 1,
                "packs": {"com.example.pack": "a".repeat(64)}
            }))
            .unwrap(),
        )
        .unwrap();
        assert!(read_index(home.path()).is_ok());
    }
}
