use std::fs;
use std::path::Path;
use std::process::Command;

const TARGETS: [&str; 5] = [
    "x86_64-unknown-linux-gnu",
    "aarch64-unknown-linux-gnu",
    "aarch64-apple-darwin",
    "x86_64-pc-windows-msvc",
    "aarch64-pc-windows-msvc",
];

pub fn validate(repo_root: &Path) -> Vec<String> {
    let mut errors = Vec::new();
    validate_release_workflow(repo_root, &mut errors);
    validate_package_boundary(repo_root, &mut errors);
    validate_python_retirement(repo_root, &mut errors);
    errors
}

fn validate_release_workflow(repo_root: &Path, errors: &mut Vec<String>) {
    let path = repo_root.join(".github/workflows/release-publish.yml");
    let text = match fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) => {
            errors.push(format!("Unable to read {}: {error}", path.display()));
            return;
        }
    };
    for target in TARGETS {
        require(&text, target, "release-publish.yml", errors);
    }
    for expected in [
        "git-slop-${RELEASE_TAG}-${TARGET}.tar.gz",
        "git-slop-${env:RELEASE_TAG}-${env:TARGET}.zip",
        "os: ubuntu-22.04-arm",
        "          - os: windows-11-arm\n            target: aarch64-pc-windows-msvc\n            archive: zip",
        "dist/SHA256SUMS",
        "dist/release-manifest.json",
        "gh release upload",
        "cargo publish -p git-slop --dry-run --locked",
        "cargo xtask release-prepare",
        "cargo xtask release-manifest",
        "node --test action/*.test.mjs",
        "Published release already exists and exactly verifies",
        "steps.release-state.outputs.published != 'true'",
        "release-verification/regenerated/release-manifest.json",
        "Create or refresh draft release assets",
        "Verify the Action installer against published or draft assets",
        "node action/install.mjs",
        "gh release delete-asset \"$release_tag\" \"$asset_name\" --yes",
        "test \"$(jq -r '.draft' <<< \"$release_json\")\" = \"true\"",
        "test \"$(gh api \"$endpoint\" --jq '.draft')\" = \"true\"",
    ] {
        require(&text, expected, "release-publish.yml", errors);
    }
    for forbidden in [
        "x86_64-apple-darwin",
        "macos-15-intel",
        "os: ubuntu-24.04-arm",
        "--clobber",
        "cargo publish --locked\n",
        "uv build",
        "scripts/build_release_manifest.py",
        "scripts/release_prepare.py",
    ] {
        forbid(&text, forbidden, "release-publish.yml", errors);
    }
    if text.matches("os: ubuntu-22.04").count() != 2 {
        errors.push("release-publish.yml must use ubuntu-22.04 exactly twice.".into());
    }
}

fn validate_package_boundary(repo_root: &Path, errors: &mut Vec<String>) {
    let root_manifest = repo_root.join("Cargo.toml");
    let xtask_manifest = repo_root.join("xtask/Cargo.toml");
    let root = match fs::read_to_string(&root_manifest) {
        Ok(text) => text,
        Err(error) => {
            errors.push(format!("Unable to read Cargo.toml: {error}"));
            return;
        }
    };
    let xtask = match fs::read_to_string(&xtask_manifest) {
        Ok(text) => text,
        Err(error) => {
            errors.push(format!("Unable to read xtask/Cargo.toml: {error}"));
            return;
        }
    };
    require(&root, "members = [\".\"]", "Cargo.toml", errors);
    require(&root, "default-members = [\".\"]", "Cargo.toml", errors);
    require(&root, "exclude = [\"xtask\"]", "Cargo.toml", errors);
    if root.lines().any(|line| {
        let line = line.trim();
        line.starts_with('"') && line.contains("xtask")
    }) {
        errors
            .push("The public git-slop package include list must not contain xtask paths.".into());
    }
    require(&xtask, "publish = false", "xtask/Cargo.toml", errors);
    require(&xtask, "[workspace]", "xtask/Cargo.toml", errors);
    if repo_root.join("Cargo.lock").exists()
        && fs::read_to_string(repo_root.join("Cargo.lock"))
            .is_ok_and(|lock| lock.contains("name = \"git-slop-xtask\""))
    {
        errors.push("The public Cargo.lock must not include git-slop-xtask.".into());
    }
    let xtask_lock = repo_root.join("xtask/Cargo.lock");
    if !xtask_lock.exists() {
        errors.push("xtask/Cargo.lock must be committed for the private tooling workspace.".into());
    }
}

fn validate_python_retirement(repo_root: &Path, errors: &mut Vec<String>) {
    for removed in ["pyproject.toml", "uv.lock", "src/git_slop"] {
        if repo_root.join(removed).exists() {
            errors.push(format!(
                "{removed} must be removed after the Rust xtask migration."
            ));
        }
    }
    match repository_python_files(repo_root) {
        Ok(paths) => errors.extend(
            paths
                .into_iter()
                .map(|path| format!("Repository-owned Python file must be removed: {path}.")),
        ),
        Err(error) => errors.push(error),
    }
}

pub fn repository_python_files(repo_root: &Path) -> Result<Vec<String>, String> {
    let output = Command::new("git")
        .args([
            "ls-files",
            "--cached",
            "--others",
            "--exclude-standard",
            "-z",
        ])
        .current_dir(repo_root)
        .output()
        .map_err(|error| format!("Unable to enumerate repository files with git: {error}"))?;
    if !output.status.success() {
        let detail = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        return Err(format!(
            "Unable to enumerate repository files with git ({}): {detail}",
            output.status
        ));
    }

    let mut paths = Vec::new();
    for raw_path in output
        .stdout
        .split(|byte| *byte == 0)
        .filter(|path| !path.is_empty())
    {
        let relative = std::str::from_utf8(raw_path).map_err(|_| {
            "Repository contains a non-UTF-8 path; Python retirement cannot be verified safely."
                .to_owned()
        })?;
        let path = repo_root.join(relative);
        if fs::symlink_metadata(&path).is_ok()
            && path.extension().and_then(|extension| extension.to_str()) == Some("py")
        {
            paths.push(relative.replace('\\', "/"));
        }
    }
    paths.sort();
    paths.dedup();
    Ok(paths)
}

fn require(text: &str, expected: &str, label: &str, errors: &mut Vec<String>) {
    if !text.contains(expected) {
        errors.push(format!("{label} must include {expected}."));
    }
}

fn forbid(text: &str, forbidden: &str, label: &str, errors: &mut Vec<String>) {
    if text.contains(forbidden) {
        errors.push(format!("{label} must not include {forbidden}."));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn supported_target_set_is_exact() {
        assert_eq!(
            TARGETS,
            [
                "x86_64-unknown-linux-gnu",
                "aarch64-unknown-linux-gnu",
                "aarch64-apple-darwin",
                "x86_64-pc-windows-msvc",
                "aarch64-pc-windows-msvc",
            ]
        );
    }

    #[test]
    fn repository_distribution_contract_passes() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
        assert_eq!(validate(root), Vec::<String>::new());
    }

    #[test]
    fn python_retirement_covers_the_entire_owned_repository() {
        let temp = TempDir::new().unwrap();
        let root = temp.path();
        assert!(
            Command::new("git")
                .args(["init", "--quiet"])
                .current_dir(root)
                .status()
                .unwrap()
                .success()
        );
        fs::create_dir_all(root.join(".github")).unwrap();
        fs::create_dir_all(root.join("ignored")).unwrap();
        fs::write(root.join(".gitignore"), "/ignored/\n").unwrap();
        fs::write(root.join("root_helper.py"), "pass\n").unwrap();
        fs::write(root.join(".github/contract.py"), "pass\n").unwrap();
        fs::write(root.join("ignored/external.py"), "pass\n").unwrap();

        assert_eq!(
            repository_python_files(root).unwrap(),
            [".github/contract.py", "root_helper.py"]
        );
    }
}
