use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};
use serde_json::{Value, json};

use crate::manifest::resolve_project_path;

fn package_purl(package: &Value) -> String {
    format!(
        "pkg:cargo/{}@{}",
        package["name"].as_str().unwrap_or_default(),
        package["version"].as_str().unwrap_or_default()
    )
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
    let components = packages
        .iter()
        .map(|package| {
            let mut component = json!({
                "type": if package["name"] == "git-slop" { "application" } else { "library" },
                "bom-ref": package["id"],
                "name": package["name"],
                "version": package["version"],
                "licenses": [],
                "purl": package_purl(package),
            });
            if let Some(license) = package["license"].as_str() {
                component["licenses"] = json!([{"license": {"id": license}}]);
            }
            component
        })
        .collect::<Vec<_>>();
    let mut nodes = metadata
        .pointer("/resolve/nodes")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    nodes.sort_by(|left, right| left["id"].as_str().cmp(&right["id"].as_str()));
    let dependencies = nodes
        .iter()
        .map(|node| {
            let mut dependencies = node["dependencies"].as_array().cloned().unwrap_or_default();
            dependencies.sort_by(|left, right| left.as_str().cmp(&right.as_str()));
            json!({"ref": node["id"], "dependsOn": dependencies})
        })
        .collect::<Vec<_>>();
    let project_component = components
        .iter()
        .find(|component| component["name"] == "git-slop")
        .cloned()
        .context("component projection omitted git-slop")?;
    let cyclonedx = json!({
        "bomFormat": "CycloneDX",
        "specVersion": "1.5",
        "serialNumber": "urn:uuid:00000000-0000-0000-0000-000000000000",
        "version": 1,
        "metadata": {"component": project_component},
        "components": components,
        "dependencies": dependencies,
    });
    let spdx_packages = packages
        .iter()
        .enumerate()
        .map(|(index, package)| {
            let license = package["license"].as_str().unwrap_or("NOASSERTION");
            json!({
                "SPDXID": format!("SPDXRef-Package-{}", index + 1),
                "name": package["name"],
                "versionInfo": package["version"],
                "downloadLocation": package["source"].as_str().unwrap_or("NOASSERTION"),
                "filesAnalyzed": false,
                "licenseConcluded": license,
                "licenseDeclared": license,
                "externalRefs": [{"referenceCategory":"PACKAGE-MANAGER","referenceType":"purl","referenceLocator":package_purl(package)}]
            })
        })
        .collect::<Vec<_>>();
    let spdx = json!({
        "spdxVersion": "SPDX-2.3",
        "dataLicense": "CC0-1.0",
        "SPDXID": "SPDXRef-DOCUMENT",
        "name": "git-slop",
        "documentNamespace": format!("https://github.com/coreycoto/git-slop/sbom/v{}", project["version"].as_str().unwrap_or_default()),
        "creationInfo": {"created":"1970-01-01T00:00:00Z","creators":["Tool: cargo-xtask-sbom"]},
        "packages": spdx_packages,
    });
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

    use super::generate;

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
    }
    use std::path::Path;
}
