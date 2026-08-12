use super::{
    CHECKSUM_FILE_NAME, InstallInstructions, RELEASE_TARGETS, REPO_FULL_NAME, artifact_name,
};

pub(super) fn install_instructions(tag: &str) -> InstallInstructions {
    let version = tag.strip_prefix('v').unwrap_or(tag);
    // v0.12.0 is already immutable on crates.io and its signed tag contains
    // the original Action verifier. Preserve that exact historical contract;
    // every later release receives the hardened instructions below.
    let legacy_tagged_action = tag == "v0.12.0";
    InstallInstructions {
        attestation: if legacy_tagged_action {
            vec![format!(
                "gh attestation verify 'git-slop-{tag}-<target>.*' --repo {REPO_FULL_NAME} --signer-repo {REPO_FULL_NAME}"
            )]
        } else {
            RELEASE_TARGETS
                .iter()
                .map(|target| {
                    format!(
                        "gh attestation verify '{}' --repo {REPO_FULL_NAME} --signer-repo {REPO_FULL_NAME}",
                        artifact_name(tag, *target)
                    )
                })
                .collect()
        },
        cargo: vec![format!(
            "cargo install git-slop --version {version} --locked"
        )],
        homebrew_tap: vec![
            "brew tap coreycoto/tap".to_owned(),
            "brew install coreycoto/tap/git-slop".to_owned(),
        ],
        github_release: github_release_instructions(tag, legacy_tagged_action),
        scoop: vec![
            "scoop bucket add coreycoto https://github.com/coreycoto/scoop-bucket".to_owned(),
            "scoop install coreycoto/git-slop".to_owned(),
        ],
    }
}

fn github_release_instructions(tag: &str, legacy_tagged_action: bool) -> Vec<String> {
    let download = format!(
        "gh release download {tag} --repo {REPO_FULL_NAME} --pattern \
         'git-slop-{tag}-<target>.*' --pattern {CHECKSUM_FILE_NAME}"
    );
    let checksum = format!("sha256sum --check {CHECKSUM_FILE_NAME} --ignore-missing");
    if legacy_tagged_action {
        vec![download, checksum]
    } else {
        vec![
            format!("gh release verify {tag} --repo {REPO_FULL_NAME}"),
            download,
            checksum,
        ]
    }
}
