const LEGACY_TAGGED_ACTION_VERSION = "0.12.0";

function downloadInstructions(tag, releaseRepository) {
  return [
    `gh release download ${tag} --repo ${releaseRepository} --pattern 'git-slop-${tag}-<target>.*' --pattern SHA256SUMS`,
    "sha256sum --check SHA256SUMS --ignore-missing",
  ];
}

export function expectedInstallInstructions({ artifacts, releaseRepository, tag, version }) {
  const githubRelease = downloadInstructions(tag, releaseRepository);

  // v0.12.0 was signed and published to crates.io before its tagged Action
  // learned the hardened install-instruction contract. Keep this one
  // immutable release self-installable without relaxing later releases.
  if (version === LEGACY_TAGGED_ACTION_VERSION) {
    return {
      attestation: [
        `gh attestation verify 'git-slop-${tag}-<target>.*' --repo ${releaseRepository} --signer-repo ${releaseRepository}`,
      ],
      githubRelease,
    };
  }

  return {
    attestation: artifacts.map(
      (artifact) =>
        `gh attestation verify '${artifact.name}' --repo ${releaseRepository} --signer-repo ${releaseRepository}`,
    ),
    githubRelease: [`gh release verify ${tag} --repo ${releaseRepository}`, ...githubRelease],
  };
}
