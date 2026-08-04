use std::env;
use std::fs;
use std::path::Path;

use serde_json::Value;

fn valid_revision(revision: &str) -> bool {
    revision.len() == 40
        && revision
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
}

fn explicit_identity() -> Result<Option<(String, bool)>, String> {
    let Ok(revision) = env::var("GIT_SLOP_SOURCE_REVISION") else {
        return Ok(None);
    };
    let revision = revision.trim();
    if revision.is_empty() {
        return Ok(None);
    }
    if !valid_revision(revision) {
        return Err("GIT_SLOP_SOURCE_REVISION must be a full lowercase commit id".into());
    }
    let dirty = env::var("GIT_SLOP_SOURCE_DIRTY")
        .map_err(|_| "GIT_SLOP_SOURCE_DIRTY is required with GIT_SLOP_SOURCE_REVISION")?
        .parse::<bool>()
        .map_err(|_| "GIT_SLOP_SOURCE_DIRTY must be true or false")?;
    Ok(Some((revision.to_owned(), dirty)))
}

fn packaged_identity() -> Result<Option<(String, bool)>, String> {
    if !Path::new(".cargo_vcs_info.json").is_file() {
        return Ok(None);
    }
    let bytes = fs::read(".cargo_vcs_info.json")
        .map_err(|error| format!("unable to read .cargo_vcs_info.json: {error}"))?;
    let payload: Value = serde_json::from_slice(&bytes)
        .map_err(|error| format!("unable to parse .cargo_vcs_info.json: {error}"))?;
    let git = payload
        .get("git")
        .and_then(Value::as_object)
        .ok_or_else(|| ".cargo_vcs_info.json must define git metadata".to_owned())?;
    let revision = git
        .get("sha1")
        .and_then(Value::as_str)
        .ok_or_else(|| ".cargo_vcs_info.json must define git.sha1".to_owned())?
        .trim();
    if !valid_revision(revision) {
        return Err(".cargo_vcs_info.json git.sha1 must be a full lowercase commit id".into());
    }
    let dirty = match git.get("dirty") {
        None => false,
        Some(value) => value.as_bool().ok_or_else(|| {
            ".cargo_vcs_info.json git.dirty must be boolean when present".to_owned()
        })?,
    };
    Ok(Some((revision.to_owned(), dirty)))
}

fn main() {
    println!("cargo::rerun-if-env-changed=GIT_SLOP_SOURCE_REVISION");
    println!("cargo::rerun-if-env-changed=GIT_SLOP_SOURCE_DIRTY");
    println!("cargo::rerun-if-changed=.cargo_vcs_info.json");

    let identity = explicit_identity()
        .and_then(|identity| match identity {
            Some(identity) => Ok(Some(identity)),
            None => packaged_identity(),
        })
        .unwrap_or_else(|error| panic!("invalid git-slop build provenance: {error}"));
    let (revision, dirty) = identity
        .map(|(revision, dirty)| (revision, Some(dirty)))
        .unwrap_or_default();
    println!("cargo::rustc-env=GIT_SLOP_SOURCE_REVISION={revision}");
    println!(
        "cargo::rustc-env=GIT_SLOP_SOURCE_DIRTY={}",
        dirty.map(|value| value.to_string()).unwrap_or_default()
    );
}
