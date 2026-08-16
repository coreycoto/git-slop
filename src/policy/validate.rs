use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Component, Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde_json::json;
use sha2::{Digest, Sha256};
use unicode_normalization::UnicodeNormalization;

use super::model::{
    CompiledPolicySet, EntrypointLock, FileDigest, PackLock, PackManifest, PolicyConflict,
    ResolvedPack,
};
use super::{CORE_PACK_ID, POLICY_LOCK_SCHEMA_VERSION, POLICY_SCHEMA_VERSION};

const MAX_FILES: usize = 64;
const MAX_TOTAL_BYTES: usize = 1_048_576;
const MAX_FILE_BYTES: usize = 262_144;
const MAX_RULES: usize = 64;
const MAX_ENTRYPOINTS: usize = 16;
const MAX_TESTS: usize = 32;

fn normalized_text(bytes: Vec<u8>, label: &str) -> Result<String> {
    let text = String::from_utf8(bytes).with_context(|| format!("{label} must be UTF-8"))?;
    if text.contains('\0') {
        bail!("{label} must not contain NUL bytes");
    }
    let text = text.replace("\r\n", "\n").replace('\r', "\n");
    if text.nfc().collect::<String>() != text {
        bail!("{label} must use Unicode NFC normalization");
    }
    Ok(text)
}

fn safe_relative_path(value: &str, prefix: &str, extensions: &[&str]) -> Result<PathBuf> {
    let path = Path::new(value);
    if value.is_empty()
        || path.is_absolute()
        || path.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
        || !value.replace('\\', "/").starts_with(prefix)
        || !extensions
            .iter()
            .any(|extension| value.ends_with(extension))
    {
        bail!("unsafe policy-pack path {value:?}");
    }
    Ok(path.to_path_buf())
}

fn valid_identifier(value: &str) -> bool {
    value.len() <= 160
        && value.contains('.')
        && value.split('.').all(|segment| {
            !segment.is_empty()
                && segment.len() <= 64
                && segment.chars().all(|character| {
                    character.is_ascii_lowercase() || character.is_ascii_digit() || character == '-'
                })
                && !segment.starts_with('-')
                && !segment.ends_with('-')
        })
}

fn strict_semver(value: &str) -> Option<[u64; 3]> {
    let parts = value
        .split('.')
        .map(str::parse::<u64>)
        .collect::<Result<Vec<_>, _>>()
        .ok()?;
    (parts.len() == 3).then(|| [parts[0], parts[1], parts[2]])
}

fn collect_pack_files(root: &Path, directory: &Path, files: &mut Vec<PathBuf>) -> Result<()> {
    for entry in fs::read_dir(directory).with_context(|| {
        format!(
            "unable to read policy-pack directory {}",
            directory.display()
        )
    })? {
        let entry = entry?;
        if entry.file_name() == ".git" {
            continue;
        }
        let metadata = fs::symlink_metadata(entry.path())?;
        if metadata.file_type().is_symlink() {
            bail!(
                "policy packs must not contain symlinks: {}",
                entry.path().display()
            );
        }
        if metadata.is_dir() {
            let relative = entry.path().strip_prefix(root)?.to_path_buf();
            if relative.components().count() > 4 {
                bail!(
                    "policy-pack directory depth exceeds four: {}",
                    relative.display()
                );
            }
            collect_pack_files(root, &entry.path(), files)?;
        } else if metadata.is_file() {
            files.push(entry.path());
            if files.len() > MAX_FILES {
                bail!("policy pack exceeds the {MAX_FILES}-file limit");
            }
        } else {
            bail!(
                "unsupported policy-pack filesystem entry: {}",
                entry.path().display()
            );
        }
    }
    Ok(())
}

fn read_pack_text(root: &Path, relative: &Path) -> Result<String> {
    let path = root.join(relative);
    let metadata = fs::symlink_metadata(&path)
        .with_context(|| format!("missing declared policy-pack file {}", relative.display()))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        bail!(
            "declared policy-pack path must be a regular file: {}",
            relative.display()
        );
    }
    if metadata.len() > MAX_FILE_BYTES as u64 {
        bail!(
            "policy-pack file exceeds {MAX_FILE_BYTES} bytes: {}",
            relative.display()
        );
    }
    normalized_text(fs::read(&path)?, &relative.to_string_lossy())
}

fn digest_file(path: &str, text: &str) -> FileDigest {
    FileDigest {
        path: path.to_string(),
        sha256: hex::encode(Sha256::digest(text.as_bytes())),
        bytes: text.len(),
    }
}

fn validate_manifest(manifest: &PackManifest, built_in: bool) -> Result<()> {
    if manifest.schema_version != POLICY_SCHEMA_VERSION {
        bail!(
            "unsupported policy-pack schema version {}",
            manifest.schema_version
        );
    }
    if !valid_identifier(&manifest.id) {
        bail!("policy-pack id must be a lowercase repository-qualified identifier");
    }
    if built_in {
        if manifest.id != CORE_PACK_ID {
            bail!("the built-in policy pack must use id {CORE_PACK_ID}");
        }
    } else if manifest.id == CORE_PACK_ID || manifest.id.starts_with(&format!("{CORE_PACK_ID}.")) {
        bail!("third-party policy packs cannot use the reserved core namespace");
    }
    if manifest.name.trim().is_empty()
        || manifest.description.trim().is_empty()
        || manifest.license.trim().is_empty()
    {
        bail!("policy-pack name, description, and license must be non-empty");
    }
    strict_semver(&manifest.version)
        .ok_or_else(|| anyhow::anyhow!("policy-pack version must be strict semver"))?;
    let minimum = strict_semver(&manifest.min_git_slop_version)
        .ok_or_else(|| anyhow::anyhow!("min_git_slop_version must be strict semver"))?;
    let runtime = strict_semver(crate::VERSION).expect("Cargo package version is strict semver");
    if runtime < minimum {
        bail!(
            "policy pack {} {} requires git-slop {} or newer (current {})",
            manifest.id,
            manifest.version,
            manifest.min_git_slop_version,
            crate::VERSION
        );
    }
    if manifest.entrypoints.is_empty() || manifest.entrypoints.len() > MAX_ENTRYPOINTS {
        bail!("policy pack must declare 1 through {MAX_ENTRYPOINTS} entrypoints");
    }
    if manifest.tests.len() > MAX_TESTS {
        bail!("policy pack exceeds the {MAX_TESTS}-test-file limit");
    }
    if manifest.rules.is_empty() || manifest.rules.len() > MAX_RULES {
        bail!("policy pack must define 1 through {MAX_RULES} rules");
    }
    let allowed_applicability = ["advise", "plan"];
    if manifest.applicability.is_empty()
        || manifest
            .applicability
            .iter()
            .any(|value| !allowed_applicability.contains(&value.as_str()))
    {
        bail!("policy-pack applicability may contain only advise and plan");
    }
    let mut ids = BTreeSet::new();
    for rule in &manifest.rules {
        if !valid_identifier(&rule.id) || !rule.id.starts_with(&format!("{}.", manifest.id)) {
            bail!(
                "rule id {} must be inside pack namespace {}",
                rule.id,
                manifest.id
            );
        }
        if !ids.insert(&rule.id) {
            bail!("duplicate policy rule id {}", rule.id);
        }
        if rule.text.trim().is_empty() || !["warning", "error"].contains(&rule.severity.as_str()) {
            bail!(
                "rule {} must define text and warning or error severity",
                rule.id
            );
        }
        if rule.applicability.is_empty()
            || rule
                .applicability
                .iter()
                .any(|value| !allowed_applicability.contains(&value.as_str()))
        {
            bail!("rule {} has unsupported applicability", rule.id);
        }
        if rule.required_evidence.is_empty() {
            bail!("rule {} must declare required_evidence", rule.id);
        }
    }
    Ok(())
}

fn resolved_from_parts(
    root: PathBuf,
    manifest_text: String,
    manifest: PackManifest,
    entrypoint_text: Vec<(String, String)>,
    test_text: Vec<(String, String)>,
    built_in: bool,
) -> Result<ResolvedPack> {
    validate_manifest(&manifest, built_in)?;
    let entrypoint_digests = entrypoint_text
        .iter()
        .map(|(path, text)| digest_file(path, text))
        .collect::<Vec<_>>();
    let test_digests = test_text
        .iter()
        .map(|(path, text)| digest_file(path, text))
        .collect::<Vec<_>>();
    let digest_payload = json!({
        "manifest": serde_yaml::from_str::<serde_json::Value>(&manifest_text)?,
        "entrypoints": &entrypoint_text,
        "tests": &test_text,
    });
    // serde_json uses ordered maps in this build; serializing this normalized
    // value is the policy-pack canonicalization contract for schema v1.
    let digest_bytes = serde_json::to_vec(&digest_payload)?;
    let content_digest = hex::encode(Sha256::digest(digest_bytes));
    Ok(ResolvedPack {
        source_type: if built_in {
            "built-in"
        } else {
            "local-directory"
        }
        .to_string(),
        source_revision: content_digest.clone(),
        manifest,
        root,
        content_digest,
        entrypoint_digests,
        test_digests,
        test_text,
        built_in,
    })
}

pub fn load_and_validate_pack(root: &Path) -> Result<ResolvedPack> {
    let root = root
        .canonicalize()
        .with_context(|| format!("unable to resolve policy-pack directory {}", root.display()))?;
    if !root.is_dir() {
        bail!("policy-pack path is not a directory: {}", root.display());
    }
    let mut files = Vec::new();
    collect_pack_files(&root, &root, &mut files)?;
    files.sort();
    let total_bytes = files.iter().try_fold(0usize, |total, path| {
        let size = usize::try_from(fs::metadata(path)?.len()).unwrap_or(usize::MAX);
        if size > MAX_FILE_BYTES {
            bail!(
                "policy-pack file exceeds {MAX_FILE_BYTES} bytes: {}",
                path.display()
            );
        }
        total
            .checked_add(size)
            .ok_or_else(|| anyhow::anyhow!("policy-pack size overflow"))
    })?;
    if total_bytes > MAX_TOTAL_BYTES {
        bail!("policy pack exceeds the {MAX_TOTAL_BYTES}-byte limit");
    }
    let manifest_text = read_pack_text(&root, Path::new("git-slop-policy.yaml"))?;
    let manifest_value: serde_json::Value =
        serde_yaml::from_str(&manifest_text).context("unable to parse git-slop-policy.yaml")?;
    let schema: serde_json::Value =
        serde_json::from_str(include_str!("../../schemas/policy-pack-1.json"))?;
    let validator = jsonschema::draft202012::options()
        .build(&schema)
        .context("embedded policy-pack schema is invalid")?;
    if let Some(error) = validator.iter_errors(&manifest_value).next() {
        bail!(
            "git-slop-policy.yaml does not match policy-pack schema v1 at {}: {}",
            error.instance_path(),
            error
        );
    }
    let manifest: PackManifest = serde_json::from_value(manifest_value)?;
    validate_manifest(&manifest, false)?;
    let entrypoints = manifest
        .entrypoints
        .iter()
        .map(|path| {
            let relative = safe_relative_path(path, "policies/", &[".md"])?;
            Ok((path.clone(), read_pack_text(&root, &relative)?))
        })
        .collect::<Result<Vec<_>>>()?;
    let tests = manifest
        .tests
        .iter()
        .map(|path| {
            let relative = safe_relative_path(path, "tests/", &[".yaml", ".yml"])?;
            Ok((path.clone(), read_pack_text(&root, &relative)?))
        })
        .collect::<Result<Vec<_>>>()?;
    let allowed = std::iter::once("git-slop-policy.yaml".to_string())
        .chain(manifest.entrypoints.iter().cloned())
        .chain(manifest.tests.iter().cloned())
        .chain([
            "README.md".to_string(),
            "LICENSE".to_string(),
            "LICENCE".to_string(),
        ])
        .collect::<BTreeSet<_>>();
    for file in files {
        let relative = file
            .strip_prefix(&root)?
            .to_string_lossy()
            .replace('\\', "/");
        if !allowed.contains(&relative) {
            bail!("undeclared policy-pack file is not allowed in schema v1: {relative}");
        }
        let _ = read_pack_text(&root, Path::new(&relative))?;
    }
    resolved_from_parts(root, manifest_text, manifest, entrypoints, tests, false)
}

pub fn core_pack() -> Result<ResolvedPack> {
    let manifest_text = include_str!("../../policies/core/git-slop-policy.yaml").to_string();
    let manifest_value: serde_json::Value = serde_yaml::from_str(&manifest_text)?;
    let schema: serde_json::Value =
        serde_json::from_str(include_str!("../../schemas/policy-pack-1.json"))?;
    let validator = jsonschema::draft202012::options()
        .build(&schema)
        .context("embedded policy-pack schema is invalid")?;
    if let Some(error) = validator.iter_errors(&manifest_value).next() {
        bail!(
            "built-in policy pack does not match schema v1 at {}: {}",
            error.instance_path(),
            error
        );
    }
    let manifest: PackManifest = serde_json::from_value(manifest_value)?;
    let entrypoints = vec![(
        "policies/invariants.md".to_string(),
        include_str!("../../policies/core/policies/invariants.md").to_string(),
    )];
    let tests = vec![(
        "tests/conformance.yaml".to_string(),
        include_str!("../../policies/core/tests/conformance.yaml").to_string(),
    )];
    resolved_from_parts(
        PathBuf::new(),
        manifest_text,
        manifest,
        entrypoints,
        tests,
        true,
    )
}

fn pack_lock(pack: &ResolvedPack) -> PackLock {
    PackLock {
        id: pack.manifest.id.clone(),
        version: pack.manifest.version.clone(),
        schema_version: pack.manifest.schema_version,
        source_type: pack.source_type.clone(),
        source_revision: pack.source_revision.clone(),
        content_digest: pack.content_digest.clone(),
        entrypoints: pack
            .entrypoint_digests
            .iter()
            .map(|entrypoint| EntrypointLock {
                path: entrypoint.path.clone(),
                sha256: entrypoint.sha256.clone(),
            })
            .collect(),
    }
}

pub fn compile_packs(mut packs: Vec<ResolvedPack>) -> Result<CompiledPolicySet> {
    if !packs
        .iter()
        .any(|pack| pack.built_in && pack.manifest.id == CORE_PACK_ID)
    {
        packs.push(core_pack()?);
    }
    packs.sort_by(|left, right| {
        right
            .built_in
            .cmp(&left.built_in)
            .then_with(|| left.manifest.id.cmp(&right.manifest.id))
    });
    let mut pack_ids = BTreeSet::new();
    let mut rules = Vec::new();
    let mut rule_ids = BTreeSet::new();
    for pack in &packs {
        if !pack_ids.insert(pack.manifest.id.clone()) {
            bail!("duplicate selected policy-pack id {}", pack.manifest.id);
        }
        for rule in &pack.manifest.rules {
            if !rule_ids.insert(rule.id.clone()) {
                bail!("duplicate selected policy rule id {}", rule.id);
            }
            rules.push(rule.clone());
        }
    }
    let by_id = rules
        .iter()
        .map(|rule| (rule.id.as_str(), rule))
        .collect::<BTreeMap<_, _>>();
    let mut conflicts = Vec::new();
    let mut seen_conflicts = BTreeSet::new();
    for rule in &rules {
        for other in &rule.conflicts_with {
            if by_id.contains_key(other.as_str()) {
                let pair = if rule.id < *other {
                    (rule.id.clone(), other.clone())
                } else {
                    (other.clone(), rule.id.clone())
                };
                if seen_conflicts.insert(pair.clone()) {
                    conflicts.push(PolicyConflict {
                        left_rule_id: pair.0,
                        right_rule_id: pair.1,
                    });
                }
            }
        }
    }
    let locks = packs.iter().map(pack_lock).collect::<Vec<_>>();
    let resolution_digest = hex::encode(Sha256::digest(serde_json::to_vec(&locks)?));
    Ok(CompiledPolicySet {
        schema_version: POLICY_LOCK_SCHEMA_VERSION,
        resolution_digest,
        packs: locks,
        rules,
        conflicts,
    })
}

#[cfg(test)]
include!("validate/tests.rs");
