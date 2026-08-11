use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::manifest::resolve_project_path;

fn package_purl(package: &Value) -> String {
    format!(
        "pkg:cargo/{}@{}",
        package["name"].as_str().unwrap_or_default(),
        package["version"].as_str().unwrap_or_default()
    )
}

fn component_ref(package: &Value) -> String {
    package_purl(package)
}

fn deterministic_uuid(seed: &[u8]) -> String {
    let mut bytes = Sha256::digest(seed)[..16].to_vec();
    bytes[6] = (bytes[6] & 0x0f) | 0x50;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    let hex = hex::encode(bytes);
    format!(
        "{}-{}-{}-{}-{}",
        &hex[..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..]
    )
}

fn release_timestamp(repo_root: &Path) -> String {
    Command::new("git")
        .current_dir(repo_root)
        .args(["log", "-1", "--format=%cI"])
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "1980-01-01T00:00:00Z".to_string())
}

fn dependency_scopes(nodes: &[Value]) -> BTreeMap<String, BTreeSet<String>> {
    let mut scopes = BTreeMap::new();
    for node in nodes {
        for dependency in node["deps"].as_array().into_iter().flatten() {
            let Some(id) = dependency["pkg"].as_str() else {
                continue;
            };
            let entry = scopes.entry(id.to_string()).or_insert_with(BTreeSet::new);
            for kind in dependency["dep_kinds"].as_array().into_iter().flatten() {
                entry.insert(kind["kind"].as_str().unwrap_or("runtime").to_string());
            }
        }
    }
    scopes
}

fn package_key(package: &Value) -> String {
    format!(
        "{}\0{}\0{}",
        package["name"].as_str().unwrap_or_default(),
        package["version"].as_str().unwrap_or_default(),
        package["source"].as_str().unwrap_or_default()
    )
}

fn cargo_lock_checksums(repo_root: &Path) -> Result<BTreeMap<String, String>> {
    let source = fs::read_to_string(repo_root.join("Cargo.lock"))
        .context("unable to read Cargo.lock for SBOM checksums")?;
    let lock: toml::Value = toml::from_str(&source).context("Cargo.lock is not valid TOML")?;
    let mut checksums = BTreeMap::new();
    for package in lock
        .get("package")
        .and_then(toml::Value::as_array)
        .into_iter()
        .flatten()
    {
        let Some(name) = package.get("name").and_then(toml::Value::as_str) else {
            continue;
        };
        let Some(version) = package.get("version").and_then(toml::Value::as_str) else {
            continue;
        };
        let source = package
            .get("source")
            .and_then(toml::Value::as_str)
            .unwrap_or_default();
        let Some(checksum) = package.get("checksum").and_then(toml::Value::as_str) else {
            continue;
        };
        if checksum.len() == 64
            && checksum
                .chars()
                .all(|character| character.is_ascii_hexdigit())
        {
            checksums.insert(
                format!("{name}\0{version}\0{source}"),
                checksum.to_ascii_lowercase(),
            );
        }
    }
    Ok(checksums)
}

fn normalized_spdx_expression(license: &str) -> String {
    license
        .split('/')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" OR ")
}

fn cyclonedx_license(license: &str) -> Value {
    let normalized = normalized_spdx_expression(license);
    let compound = normalized.contains(" AND ")
        || normalized.contains(" OR ")
        || normalized.contains(" WITH ")
        || normalized.contains('(')
        || normalized.contains(')');
    if compound {
        json!({"expression": normalized})
    } else {
        json!({"license": {"id": normalized}})
    }
}

fn cyclonedx_scope(scopes: &[String], root: bool) -> &'static str {
    if root
        || scopes
            .iter()
            .any(|scope| scope == "runtime" || scope == "normal")
    {
        "required"
    } else if scopes.iter().any(|scope| scope == "build") {
        "optional"
    } else {
        "excluded"
    }
}

fn validate_graphs(cyclonedx: &Value, spdx: &Value) -> Result<()> {
    let component_refs = cyclonedx["components"]
        .as_array()
        .context("CycloneDX components must be an array")?
        .iter()
        .map(|component| {
            component["bom-ref"]
                .as_str()
                .context("CycloneDX component omitted bom-ref")
        })
        .collect::<Result<BTreeSet<_>>>()?;
    if component_refs.len()
        != cyclonedx["components"]
            .as_array()
            .map(Vec::len)
            .unwrap_or_default()
    {
        bail!("CycloneDX component bom-ref values must be unique");
    }
    let dependencies = cyclonedx["dependencies"]
        .as_array()
        .context("CycloneDX dependencies must be an array")?;
    for dependency in dependencies {
        let source = dependency["ref"]
            .as_str()
            .context("CycloneDX dependency omitted ref")?;
        if !component_refs.contains(source) {
            bail!("CycloneDX dependency source is not a component: {source}");
        }
        for target in dependency["dependsOn"].as_array().into_iter().flatten() {
            let target = target
                .as_str()
                .context("CycloneDX dependsOn entry must be a string")?;
            if !component_refs.contains(target) {
                bail!("CycloneDX dependency target is not a component: {target}");
            }
        }
    }

    let package_ids = spdx["packages"]
        .as_array()
        .context("SPDX packages must be an array")?
        .iter()
        .map(|package| {
            package["SPDXID"]
                .as_str()
                .context("SPDX package omitted SPDXID")
        })
        .collect::<Result<BTreeSet<_>>>()?;
    for relationship in spdx["relationships"]
        .as_array()
        .context("SPDX relationships must be an array")?
    {
        let source = relationship["spdxElementId"]
            .as_str()
            .context("SPDX relationship omitted source")?;
        let target = relationship["relatedSpdxElement"]
            .as_str()
            .context("SPDX relationship omitted target")?;
        if !package_ids.contains(source) || !package_ids.contains(target) {
            bail!("SPDX relationship references an unknown package: {source} -> {target}");
        }
    }
    if package_ids.len() > 1 && spdx["relationships"].as_array().is_none_or(Vec::is_empty) {
        bail!("SPDX dependency graph is empty despite containing dependencies");
    }
    Ok(())
}

pub fn generate(repo_root: &Path, output_dir: &Path) -> Result<Vec<PathBuf>> {
    let output = Command::new("cargo")
        .current_dir(repo_root)
        .args(["metadata", "--locked", "--format-version", "1"])
        .output()
        .context("unable to run cargo metadata for SBOM generation")?;
    if !output.status.success() {
        bail!(
            "cargo metadata failed during SBOM generation: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    let metadata: Value =
        serde_json::from_slice(&output.stdout).context("cargo metadata returned invalid JSON")?;
    let lock_checksums = cargo_lock_checksums(repo_root)?;
    let mut packages = metadata["packages"]
        .as_array()
        .cloned()
        .context("cargo metadata omitted packages")?;
    packages.sort_by(|left, right| {
        (
            left["name"].as_str(),
            left["version"].as_str(),
            left["id"].as_str(),
        )
            .cmp(&(
                right["name"].as_str(),
                right["version"].as_str(),
                right["id"].as_str(),
            ))
    });
    let project = packages
        .iter()
        .find(|package| package["name"] == "git-slop")
        .context("cargo metadata omitted git-slop")?;
    let refs = packages
        .iter()
        .filter_map(|package| Some((package["id"].as_str()?.to_string(), component_ref(package))))
        .collect::<BTreeMap<_, _>>();
    let spdx_ids = packages
        .iter()
        .enumerate()
        .filter_map(|(index, package)| {
            Some((
                package["id"].as_str()?.to_string(),
                format!("SPDXRef-Package-{}", index + 1),
            ))
        })
        .collect::<BTreeMap<_, _>>();
    let mut nodes = metadata
        .pointer("/resolve/nodes")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    nodes.sort_by(|left, right| left["id"].as_str().cmp(&right["id"].as_str()));
    let scopes = dependency_scopes(&nodes);
    let components = packages
        .iter()
        .map(|package| {
            let mut component = json!({
                "type": if package["name"] == "git-slop" { "application" } else { "library" },
                "bom-ref": component_ref(package),
                "name": package["name"],
                "version": package["version"],
                "licenses": [],
                "purl": package_purl(package),
            });
            if let Some(license) = package["license"].as_str() {
                component["licenses"] = json!([cyclonedx_license(license)]);
            }
            let package_scopes = package["id"]
                .as_str()
                .and_then(|id| scopes.get(id))
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .collect::<Vec<_>>();
            component["scope"] = json!(cyclonedx_scope(
                &package_scopes,
                package["name"] == "git-slop"
            ));
            if let Some(checksum) = lock_checksums.get(&package_key(package)) {
                component["hashes"] = json!([{"alg": "SHA-256", "content": checksum}]);
            }
            component["properties"] = json!([{
                "name": "git-slop:dependency:scopes",
                "value": if package_scopes.is_empty() { "root".to_string() } else { package_scopes.join(",") }
            }]);
            component
        })
        .collect::<Vec<_>>();
    let dependencies = nodes
        .iter()
        .map(|node| {
            let mut dependencies = node["dependencies"].as_array().cloned().unwrap_or_default();
            dependencies.sort_by(|left, right| left.as_str().cmp(&right.as_str()));
            let node_ref = node["id"]
                .as_str()
                .and_then(|id| refs.get(id))
                .cloned()
                .unwrap_or_default();
            let dependencies = dependencies
                .iter()
                .filter_map(Value::as_str)
                .filter_map(|id| refs.get(id).cloned())
                .collect::<Vec<_>>();
            json!({"ref": node_ref, "dependsOn": dependencies})
        })
        .collect::<Vec<_>>();
    let project_component = components
        .iter()
        .find(|component| component["name"] == "git-slop")
        .cloned()
        .context("component projection omitted git-slop")?;
    let seed = format!(
        "git-slop:{}",
        project["version"].as_str().unwrap_or_default()
    );
    let cyclonedx = json!({
        "bomFormat": "CycloneDX",
        "specVersion": "1.5",
        "serialNumber": format!("urn:uuid:{}", deterministic_uuid(seed.as_bytes())),
        "version": 1,
        "metadata": {
            "component": project_component,
            "properties": [
                {"name": "git-slop:sbom:kind", "value": "source"},
                {"name": "git-slop:sbom:target", "value": "all-cargo-targets"}
            ]
        },
        "components": components,
        "dependencies": dependencies,
    });
    let spdx_packages = packages
        .iter()
        .enumerate()
        .map(|(index, package)| {
            let license = package["license"]
                .as_str()
                .map(normalized_spdx_expression)
                .unwrap_or_else(|| "NOASSERTION".to_string());
            let mut value = json!({
                "SPDXID": format!("SPDXRef-Package-{}", index + 1),
                "name": package["name"],
                "versionInfo": package["version"],
                "downloadLocation": package["source"].as_str().unwrap_or("NOASSERTION"),
                "filesAnalyzed": false,
                "licenseConcluded": license,
                "licenseDeclared": license,
                "externalRefs": [{"referenceCategory":"PACKAGE-MANAGER","referenceType":"purl","referenceLocator":package_purl(package)}]
            });
            if let Some(checksum) = lock_checksums.get(&package_key(package)) {
                value["checksums"] = json!([{"algorithm": "SHA256", "checksumValue": checksum}]);
            }
            value
        })
        .collect::<Vec<_>>();
    let mut relationships = Vec::new();
    for node in &nodes {
        let Some(source) = node["id"].as_str().and_then(|id| spdx_ids.get(id)) else {
            continue;
        };
        if let Some(deps) = node["deps"].as_array() {
            for dependency in deps {
                let Some(target) = dependency["pkg"].as_str().and_then(|id| spdx_ids.get(id))
                else {
                    continue;
                };
                let mut scopes = dependency["dep_kinds"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(|kind| kind["kind"].as_str().unwrap_or("runtime"))
                    .collect::<Vec<_>>();
                scopes.sort_unstable();
                scopes.dedup();
                relationships.push(json!({
                    "spdxElementId": source,
                    "relationshipType": "DEPENDS_ON",
                    "relatedSpdxElement": target,
                    "comment": format!("dependency_scope={}", scopes.join(","))
                }));
            }
        }
    }
    relationships.sort_by(|left, right| {
        (
            left["spdxElementId"].as_str(),
            left["relatedSpdxElement"].as_str(),
        )
            .cmp(&(
                right["spdxElementId"].as_str(),
                right["relatedSpdxElement"].as_str(),
            ))
    });
    let spdx = json!({
        "spdxVersion": "SPDX-2.3",
        "dataLicense": "CC0-1.0",
        "SPDXID": "SPDXRef-DOCUMENT",
        "name": "git-slop",
        "documentNamespace": format!("https://github.com/coreycoto/git-slop/sbom/v{}", project["version"].as_str().unwrap_or_default()),
        "creationInfo": {"created":release_timestamp(repo_root),"creators":["Tool: cargo-xtask-sbom"],"comment":"source SBOM covering all Cargo targets; dependency scope is recorded per relationship"},
        "packages": spdx_packages,
        "relationships": relationships,
    });
    validate_graphs(&cyclonedx, &spdx)?;
    let output_dir = resolve_project_path(repo_root, output_dir)?;
    fs::create_dir_all(&output_dir)?;
    let outputs = [
        ("git-slop.cdx.json", cyclonedx),
        ("git-slop.spdx.json", spdx),
    ]
    .into_iter()
    .map(|(name, value)| {
        let path = output_dir.join(name);
        fs::write(&path, serde_json::to_string_pretty(&value)? + "\n")?;
        Ok(path)
    })
    .collect::<Result<Vec<_>>>()?;
    Ok(outputs)
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::{cyclonedx_license, generate, normalized_spdx_expression, validate_graphs};

    #[test]
    fn compound_and_legacy_slash_licenses_use_cyclonedx_expressions() {
        assert_eq!(
            cyclonedx_license("MIT OR Apache-2.0"),
            serde_json::json!({"expression": "MIT OR Apache-2.0"})
        );
        assert_eq!(
            cyclonedx_license("MIT/Apache-2.0"),
            serde_json::json!({"expression": "MIT OR Apache-2.0"})
        );
        assert_eq!(
            cyclonedx_license("MIT"),
            serde_json::json!({"license": {"id": "MIT"}})
        );
        assert_eq!(
            normalized_spdx_expression("BSD-3-Clause/MIT"),
            "BSD-3-Clause OR MIT"
        );
    }

    #[test]
    fn generates_deterministic_cyclonedx_and_spdx_documents() {
        let root = tempdir().unwrap();
        std::fs::write(
            root.path().join("Cargo.toml"),
            "[package]\nname = \"git-slop\"\nversion = \"0.11.0\"\nedition = \"2024\"\nlicense = \"MIT\"\n",
        )
        .unwrap();
        std::fs::write(
            root.path().join("Cargo.lock"),
            "# This file is automatically @generated by Cargo.\nversion = 4\n\n[[package]]\nname = \"git-slop\"\nversion = \"0.11.0\"\n",
        )
        .unwrap();
        std::fs::create_dir(root.path().join("src")).unwrap();
        std::fs::write(root.path().join("src/main.rs"), "fn main() {}\n").unwrap();
        let one = generate(root.path(), Path::new("first")).unwrap();
        let two = generate(root.path(), Path::new("second")).unwrap();
        assert_eq!(
            std::fs::read(&one[0]).unwrap(),
            std::fs::read(&two[0]).unwrap()
        );
        assert_eq!(
            std::fs::read(&one[1]).unwrap(),
            std::fs::read(&two[1]).unwrap()
        );
        let cyclonedx: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&one[0]).unwrap()).unwrap();
        let spdx: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&one[1]).unwrap()).unwrap();
        assert_eq!(cyclonedx["bomFormat"], "CycloneDX");
        assert_eq!(spdx["spdxVersion"], "SPDX-2.3");
        validate_graphs(&cyclonedx, &spdx).unwrap();
        assert_eq!(
            cyclonedx["metadata"]["component"]["bom-ref"],
            "pkg:cargo/git-slop@0.11.0"
        );
    }
    use std::path::Path;
}
