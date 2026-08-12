use std::env;
use std::fs;
use std::path::Path;
use std::process::Command;

use serde_json::Value;

fn valid_revision(revision: &str) -> bool {
    revision.len() == 40
        && revision
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
}

fn valid_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
}

fn crate_sha256() -> Result<String, String> {
    let value = env::var("GIT_SLOP_CRATE_SHA256").unwrap_or_default();
    let value = value.trim();
    if !value.is_empty() && !valid_sha256(value) {
        return Err("GIT_SLOP_CRATE_SHA256 must be a lowercase SHA-256 digest".into());
    }
    Ok(value.to_owned())
}

fn rustc_version() -> Result<String, String> {
    let rustc = env::var_os("RUSTC").ok_or_else(|| "RUSTC is not set".to_owned())?;
    let output = Command::new(rustc)
        .arg("--version")
        .output()
        .map_err(|error| format!("unable to execute rustc --version: {error}"))?;
    if !output.status.success() {
        return Err("rustc --version failed".into());
    }
    String::from_utf8(output.stdout)
        .map(|value| value.trim().to_owned())
        .map_err(|error| format!("rustc --version was not UTF-8: {error}"))
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
    println!("cargo::rerun-if-env-changed=GIT_SLOP_CRATE_SHA256");
    println!("cargo::rerun-if-changed=.cargo_vcs_info.json");

    let explicit = explicit_identity()
        .unwrap_or_else(|error| panic!("invalid git-slop build provenance: {error}"));
    let build_source = if explicit.is_some() {
        "release"
    } else if Path::new(".cargo_vcs_info.json").is_file() {
        "crate"
    } else {
        "workspace"
    };
    let identity = Ok(explicit)
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
    let target = env::var("TARGET").expect("Cargo must set TARGET");
    let crate_sha256 =
        crate_sha256().unwrap_or_else(|error| panic!("invalid git-slop build provenance: {error}"));
    let rustc_version = rustc_version()
        .unwrap_or_else(|error| panic!("invalid git-slop compiler provenance: {error}"));
    println!("cargo::rustc-env=GIT_SLOP_BUILD_TARGET={target}");
    println!("cargo::rustc-env=GIT_SLOP_CRATE_SHA256={crate_sha256}");
    println!("cargo::rustc-env=GIT_SLOP_RUSTC_VERSION={rustc_version}");
    println!("cargo::rustc-env=GIT_SLOP_BUILD_SOURCE={build_source}");
}
