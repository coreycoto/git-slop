mod tests {
    use super::*;
    use tempfile::TempDir;

    fn version_fixture() -> TempDir {
        let temp = TempDir::new().unwrap();
        let root = temp.path();
        fs::create_dir_all(root.join("action")).unwrap();
        fs::create_dir_all(root.join("docs")).unwrap();
        fs::create_dir_all(root.join("man")).unwrap();
        fs::create_dir_all(root.join(".github/workflows")).unwrap();
        fs::create_dir_all(root.join("plugins/git-slop/skills/adopt-repo")).unwrap();
        fs::create_dir_all(root.join("xtask")).unwrap();
        fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"git-slop\"\nversion = \"0.9.0\"\n",
        )
        .unwrap();
        fs::write(
            root.join("Cargo.lock"),
            "version = 4\n\n[[package]]\nname = \"git-slop\"\nversion = \"0.9.0\"\n",
        )
        .unwrap();
        fs::write(
            root.join("action.yml"),
            "inputs:\n  version:\n    default: \"0.9.0\"\n",
        )
        .unwrap();
        fs::write(
            root.join("action/install.mjs"),
            "const version = process.env.GIT_SLOP_ACTION_VERSION || \"0.9.0\";\n",
        )
        .unwrap();
        fs::write(
            root.join(".github/workflows/release-publish.yml"),
            "on:\n  workflow_dispatch:\n    inputs:\n      version:\n        default: \"0.9.0\"\n",
        )
        .unwrap();
        fs::write(
            root.join("README.md"),
            "uses: coreycoto/git-slop@v0.9.0\n\
             cargo install git-slop --version 0.9.0\n",
        )
        .unwrap();
        fs::write(
            root.join("docs/github-action.md"),
            "uses: coreycoto/git-slop@v0.9.0\n\n## Inputs\n\n| Input | Default | Purpose |\n| --- | --- | --- |\n| `version` | `0.9.0` | Version |\n\n## Outputs\n\n| Output | Purpose |\n| --- | --- |\n",
        )
        .unwrap();
        fs::write(
            root.join("docs/install.md"),
            "cargo install git-slop --version 0.9.0\n\
             After the bucket lists 0.9.0\n",
        )
        .unwrap();
        fs::write(root.join("docs/archive-install.md"), "release=v0.9.0\n").unwrap();
        fs::write(
            root.join("plugins/git-slop/skills/adopt-repo/SKILL.md"),
            "Minimal CI adoption after `0.9.0` is published:\nuses: coreycoto/git-slop@v0.9.0\n",
        )
        .unwrap();
        fs::write(
            root.join("xtask/README.md"),
            "cargo xtask release-prepare --version 0.9.0\ncargo xtask release-manifest --tag v0.9.0\n",
        )
        .unwrap();
        fs::write(
            root.join("man/git-slop.1"),
            ".TH GIT-SLOP 1 \"today\" \"git-slop 0.9.0\"\n",
        )
        .unwrap();
        temp
    }

    #[test]
    fn repository_distribution_contract_passes() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
        assert_eq!(validate(root), Vec::<String>::new());
    }

    #[test]
    fn advisor_feature_gate_is_parsed_structurally() {
        assert!(advisor_features_fail_closed(
            "[features]\ndefault = []\nadvisor-inference-benchmark = []\n"
        ));
        assert!(!advisor_features_fail_closed(
            "# default = []\n[features]\ndefault = [\"advisor-inference-benchmark\"]\nadvisor-inference-benchmark = []\n"
        ));
        assert!(!advisor_features_fail_closed(
            "[features]\ndefault = []\n# advisor-inference-benchmark = []\n"
        ));
    }

    #[test]
    fn version_alignment_covers_structured_and_documented_surfaces() {
        let temp = version_fixture();
        let mut errors = Vec::new();
        validate_version_alignment(temp.path(), &mut errors);
        assert_eq!(errors, Vec::<String>::new());

        fs::write(
            temp.path().join("Cargo.lock"),
            "version = 4\n\n[[package]]\nname = \"git-slop\"\nversion = \"0.9.1\"\n",
        )
        .unwrap();
        fs::write(
            temp.path().join("action.yml"),
            "inputs:\n  version:\n    default: \"0.9.1\"\n",
        )
        .unwrap();
        fs::write(
            temp.path().join("action/install.mjs"),
            "const version = process.env.GIT_SLOP_ACTION_VERSION || \"0.9.1\";\n",
        )
        .unwrap();
        fs::write(
            temp.path().join(".github/workflows/release-publish.yml"),
            "on:\n  workflow_dispatch:\n    inputs:\n      version:\n        default: \"0.9.1\"\n",
        )
        .unwrap();
        fs::write(
            temp.path().join("README.md"),
            "uses: coreycoto/git-slop@v0.9.1\n\
             cargo install git-slop --version 0.9.1\n",
        )
        .unwrap();
        fs::write(
            temp.path().join("docs/install.md"),
            "cargo install git-slop --version 0.9.1\n\
             After the bucket lists 0.9.1\n",
        )
        .unwrap();
        fs::write(
            temp.path()
                .join("plugins/git-slop/skills/adopt-repo/SKILL.md"),
            "Minimal CI adoption after `0.9.1` is published:\nuses: coreycoto/git-slop@v0.9.1\n",
        )
        .unwrap();
        fs::write(
            temp.path().join("xtask/README.md"),
            "cargo xtask release-prepare --version 0.9.1\ncargo xtask release-manifest --tag v0.9.1\n",
        )
        .unwrap();

        let mut errors = Vec::new();
        validate_version_alignment(temp.path(), &mut errors);
        let rendered = errors.join("\n");
        for expected in [
            "Cargo.lock",
            "action.yml",
            "action/install.mjs",
            ".github/workflows/release-publish.yml",
            "README.md",
            "docs/install.md",
            "plugins/git-slop/skills/adopt-repo/SKILL.md",
            "xtask/README.md",
        ] {
            assert!(
                rendered.contains(expected),
                "missing {expected}: {rendered}"
            );
        }
    }

    #[test]
    fn removed_runtime_check_covers_the_entire_owned_repository() {
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
            repository_owned_py_files(root).unwrap(),
            [".github/contract.py", "root_helper.py"]
        );
    }

    #[test]
    fn scoop_contract_stays_documented_and_external() {
        let temp = TempDir::new().unwrap();
        let root = temp.path();
        fs::create_dir_all(root.join("docs")).unwrap();
        fs::create_dir_all(root.join(".github/workflows")).unwrap();
        fs::write(
            root.join("README.md"),
            "scoop bucket add coreycoto https://github.com/coreycoto/scoop-bucket\n\
             scoop install coreycoto/git-slop\n",
        )
        .unwrap();
        fs::write(
            root.join("docs/install.md"),
            "https://github.com/coreycoto/scoop-bucket\n\
             scoop install coreycoto/git-slop\n\
             scoop update git-slop\n\
             SHA256SUMS\n\
             release-manifest.json\n",
        )
        .unwrap();
        fs::write(
            root.join("docs/release-checklist.md"),
            "## Publish And Verify The External Scoop Manifest\n\
             automatic trusted-main Scoop receiver\n\
             git-slop-v<version>-x86_64-pc-windows-msvc.zip\n\
             git-slop-v<version>-aarch64-pc-windows-msvc.zip\n\
             cross-version upgrade-in-place\n\
             scoop update git-slop\n\
             scoop uninstall git-slop\n",
        )
        .unwrap();
        fs::write(
            root.join("docs/architecture.md"),
            "coreycoto/scoop-bucket\n\
             twelve-asset/eleven-checksum\n\
             trusted-main receiver creates a manifest-only bucket pull request\n",
        )
        .unwrap();
        fs::write(
            root.join(".github/workflows/release-publish.yml"),
            "name: Release\n",
        )
        .unwrap();
        fs::write(
            root.join(".github/workflows/release-published.yml"),
            "name: Verify release\n\
             Dispatch immutable release identity to Scoop bucket\n\
             secrets.SCOOP_BUCKET_DISPATCH_TOKEN\n\
             --repo coreycoto/scoop-bucket\n\
             --field release_manifest_sha256=\n\
             .immutable == true\n",
        )
        .unwrap();

        let mut errors = Vec::new();
        validate_scoop_boundary(root, &mut errors);
        assert_eq!(errors, Vec::<String>::new());

        fs::write(
            root.join(".github/workflows/release-publish.yml"),
            "name: Dispatch Scoop update\nsecrets.SCOOP_BUCKET_DISPATCH_TOKEN\n",
        )
        .unwrap();
        fs::write(
            root.join("docs/install.md"),
            "https://github.com/coreycoto/scoop-bucket\n\
             scoop install coreycoto/git-slop\n\
             SHA256SUMS\n\
             release-manifest.json\n",
        )
        .unwrap();

        let mut errors = Vec::new();
        validate_scoop_boundary(root, &mut errors);
        let rendered = errors.join("\n");
        assert!(rendered.contains("scoop update git-slop"), "{rendered}");
        assert!(
            rendered.contains("must remain independent of the external Scoop bucket"),
            "{rendered}"
        );
    }
}
