export function createManifestVerifier({
  maximumArchiveBytes,
  maximumChecksumBytes,
  maximumFormulaBytes,
  maximumManifestBytes,
  maximumSbomBytes,
  sbomAssetNames,
  sha256,
  supportedTargets,
}) {
  function verifyReleaseAssetDigest(asset, bytes) {
    if (!/^sha256:[a-f0-9]{64}$/u.test(asset.digest || "")) {
      throw new Error(`release asset ${asset.name} has no valid GitHub SHA-256 digest`);
    }
    const actual = sha256(bytes);
    const expected = asset.digest.slice("sha256:".length);
    if (actual !== expected) {
      throw new Error(
        `GitHub release digest mismatch for ${asset.name}: expected ${expected}, received ${actual}`,
      );
    }
    return actual;
  }

  function exactObject(value, expectedKeys, label) {
    if (value === null || typeof value !== "object" || Array.isArray(value)) {
      throw new Error(`${label} must be an object`);
    }
    const actual = Object.keys(value).sort();
    const expected = [...expectedKeys].sort();
    if (actual.length !== expected.length || actual.some((key, index) => key !== expected[index])) {
      throw new Error(`${label} must contain exactly: ${expected.join(", ")}`);
    }
    return value;
  }

  function exactStringArray(value, expected, label) {
    if (
      !Array.isArray(value) ||
      value.length !== expected.length ||
      value.some((entry, index) => typeof entry !== "string" || entry !== expected[index])
    ) {
      throw new Error(`${label} does not match the canonical release instructions`);
    }
  }

  function parseChecksums(manifest) {
    const lines = manifest.split(/\r?\n/u);
    if (lines.at(-1) === "") {
      lines.pop();
    }
    if (lines.length === 0) {
      throw new Error("SHA256SUMS is empty");
    }
    const checksums = new Map();
    for (const line of lines) {
      const match = line.match(/^([a-f0-9]{64}) {2}([^/\\]+)$/u);
      if (!match) {
        throw new Error(`SHA256SUMS contains a malformed entry: ${JSON.stringify(line)}`);
      }
      const [, digest, name] = match;
      if (checksums.has(name)) {
        throw new Error(`SHA256SUMS contains a duplicate entry for ${name}`);
      }
      checksums.set(name, digest);
    }
    return checksums;
  }

  function requiredChecksum(checksums, assetName) {
    const digest = checksums.get(assetName);
    if (!digest) {
      throw new Error(`SHA256SUMS has no exact entry for ${assetName}`);
    }
    return digest;
  }

  function archiveAssetName(tag, target) {
    const metadata = supportedTargets[target];
    return `git-slop-${tag}-${target}.${metadata.archive}`;
  }

  function releaseAssetSizeLimit(name, tag) {
    if (name === "SHA256SUMS") {
      return maximumChecksumBytes;
    }
    if (name === "release-manifest.json") {
      return maximumManifestBytes;
    }
    if (name === "git-slop.rb") {
      return maximumFormulaBytes;
    }
    if (sbomAssetNames.includes(name)) {
      return maximumSbomBytes;
    }
    if (Object.keys(supportedTargets).some((target) => archiveAssetName(tag, target) === name)) {
      return maximumArchiveBytes;
    }
    throw new Error(`unexpected release asset ${name}`);
  }

  function releaseDownloadBase(release, releaseRepository, tag) {
    const releasesBase = `https://github.com/${releaseRepository}/releases`;
    if (release.draft !== true) {
      return `${releasesBase}/download/${tag}`;
    }

    const draftPagePrefix = `${releasesBase}/tag/`;
    if (
      typeof release.html_url !== "string" ||
      !release.html_url.startsWith(draftPagePrefix)
    ) {
      throw new Error(`release ${tag} contains invalid draft URL metadata`);
    }
    const draftSlug = release.html_url.slice(draftPagePrefix.length);
    if (!/^untagged-[A-Za-z0-9]+$/u.test(draftSlug)) {
      throw new Error(`release ${tag} contains invalid draft URL metadata`);
    }
    return `${releasesBase}/download/${draftSlug}`;
  }

  function exactReleaseAssets(release, releaseRepository, tag) {
    const expectedNames = new Set([
      ...Object.keys(supportedTargets).map((target) => archiveAssetName(tag, target)),
      "SHA256SUMS",
      "git-slop.rb",
      ...sbomAssetNames,
      "release-manifest.json",
    ]);
    if (!Array.isArray(release.assets) || release.assets.length !== expectedNames.size) {
      throw new Error(
        `release ${tag} must contain exactly ${expectedNames.size} distribution assets`,
      );
    }
    const downloadBase = releaseDownloadBase(release, releaseRepository, tag);
    const assets = new Map();
    for (const asset of release.assets) {
      if (asset === null || typeof asset !== "object" || Array.isArray(asset)) {
        throw new Error(`release ${tag} contains invalid asset metadata`);
      }
      if (typeof asset.name !== "string" || !expectedNames.has(asset.name)) {
        throw new Error(`release ${tag} contains unexpected asset ${JSON.stringify(asset.name)}`);
      }
      if (assets.has(asset.name)) {
        throw new Error(`release ${tag} contains duplicate asset ${asset.name}`);
      }
      const maximumBytes = releaseAssetSizeLimit(asset.name, tag);
      if (
        !Number.isSafeInteger(asset.size) ||
        asset.size <= 0 ||
        asset.size > maximumBytes ||
        !/^sha256:[a-f0-9]{64}$/u.test(asset.digest || "") ||
        typeof asset.url !== "string" ||
        asset.url.length === 0 ||
        asset.browser_download_url !== `${downloadBase}/${asset.name}`
      ) {
        throw new Error(`release ${tag} contains invalid metadata for asset ${asset.name}`);
      }
      assets.set(asset.name, asset);
    }
    if ([...expectedNames].some((name) => !assets.has(name))) {
      throw new Error(`release ${tag} is missing a required distribution asset`);
    }
    return assets;
  }

  function releaseManifestIdentity(
    bytes,
    {
      version,
      tag,
      target,
      assetName,
      releaseRepository,
      archiveSha256,
      checksums,
      releaseAssets,
      manifestSha256,
      formulaSha256,
    },
  ) {
    let manifest;
    try {
      manifest = JSON.parse(bytes.toString("utf8"));
    } catch (error) {
      throw new Error(`release-manifest.json is invalid JSON: ${error.message}`);
    }
    exactObject(
      manifest,
      [
        "artifacts",
        "checksums",
        "crate_source",
        "install",
        "project",
        "repository",
        "revision",
        "schema_version",
        "supplemental_assets",
        "tag",
        "version",
      ],
      "release-manifest.json",
    );
    const revision = manifest.revision;
    const crateSource = manifest.crate_source;
    if (
      manifest.schema_version !== 3 ||
      manifest.project !== "git-slop" ||
      manifest.repository !== releaseRepository ||
      manifest.version !== version ||
      manifest.tag !== tag ||
      !/^[a-f0-9]{40}$/u.test(revision || "")
    ) {
      throw new Error("release-manifest.json has inconsistent release identity");
    }
    exactObject(
      crateSource,
      [
        "package",
        "registry",
        "revision",
        "schema_version",
        "sha256",
        "url",
        "vcs_dirty",
        "version",
      ],
      "release-manifest.json crate_source",
    );
    if (
      crateSource.schema_version !== 1 ||
      crateSource.registry !== "crates.io" ||
      crateSource.package !== "git-slop" ||
      crateSource.version !== version ||
      crateSource.revision !== revision ||
      crateSource.vcs_dirty !== false ||
      crateSource.url !==
        `https://static.crates.io/crates/git-slop/git-slop-${version}.crate` ||
      !/^[a-f0-9]{64}$/u.test(crateSource.sha256 || "")
    ) {
      throw new Error("release-manifest.json has inconsistent crates.io provenance");
    }
    if (!Array.isArray(manifest.artifacts) || manifest.artifacts.length !== 7) {
      throw new Error("release-manifest.json must describe exactly seven release targets");
    }
    const targetSet = new Set();
    const nameSet = new Set();
    for (const candidate of manifest.artifacts) {
      exactObject(
        candidate,
        ["arch", "archive", "name", "os", "path", "sha256", "size_bytes", "target", "url"],
        "release-manifest.json artifact",
      );
      const metadata = supportedTargets[candidate.target];
      const expectedName = metadata
        ? `git-slop-${tag}-${candidate.target}.${metadata.archive}`
        : "";
      const expectedUrl = `https://github.com/${releaseRepository}/releases/download/${tag}/${expectedName}`;
      if (
        !metadata ||
        targetSet.has(candidate.target) ||
        nameSet.has(candidate.name) ||
        candidate.name !== expectedName ||
        candidate.path !== expectedName ||
        candidate.os !== metadata.os ||
        candidate.arch !== metadata.arch ||
        candidate.archive !== metadata.archive ||
        candidate.url !== expectedUrl ||
        !/^[a-f0-9]{64}$/u.test(candidate.sha256 || "") ||
        !Number.isSafeInteger(candidate.size_bytes) ||
        candidate.size_bytes <= 0 ||
        candidate.size_bytes > maximumArchiveBytes
      ) {
        throw new Error("release-manifest.json has an invalid or duplicate target artifact");
      }
      const releaseAsset = releaseAssets.get(candidate.name);
      if (
        !releaseAsset ||
        releaseAsset.size !== candidate.size_bytes ||
        releaseAsset.digest !== `sha256:${candidate.sha256}` ||
        requiredChecksum(checksums, candidate.name) !== candidate.sha256
      ) {
        throw new Error(`release-manifest.json does not authenticate ${candidate.name}`);
      }
      targetSet.add(candidate.target);
      nameSet.add(candidate.name);
    }
    if (Object.keys(supportedTargets).some((supported) => !targetSet.has(supported))) {
      throw new Error("release-manifest.json is missing a supported target artifact");
    }
    const expectedSupplemental = new Map([
      ["git-slop.rb", ["homebrew_formula", "text/x-ruby"]],
      ["git-slop.cdx.json", ["cyclonedx_sbom", "application/vnd.cyclonedx+json"]],
      ["git-slop.spdx.json", ["spdx_sbom", "application/spdx+json"]],
    ]);
    if (
      !Array.isArray(manifest.supplemental_assets) ||
      manifest.supplemental_assets.length !== expectedSupplemental.size
    ) {
      throw new Error("release-manifest.json must describe the complete supplemental asset set");
    }
    const supplementalNames = new Set();
    for (const candidate of manifest.supplemental_assets) {
      exactObject(
        candidate,
        [
          "contract_version",
          "media_type",
          "name",
          "path",
          "required",
          "role",
          "sha256",
          "size_bytes",
          "url",
        ],
        "release-manifest.json supplemental asset",
      );
      const contract = expectedSupplemental.get(candidate.name);
      const releaseAsset = releaseAssets.get(candidate.name);
      if (
        !contract ||
        supplementalNames.has(candidate.name) ||
        candidate.path !== candidate.name ||
        candidate.role !== contract[0] ||
        candidate.media_type !== contract[1] ||
        candidate.required !== true ||
        candidate.contract_version !== 1 ||
        !/^[a-f0-9]{64}$/u.test(candidate.sha256 || "") ||
        !Number.isSafeInteger(candidate.size_bytes) ||
        candidate.size_bytes <= 0 ||
        candidate.url !==
          `https://github.com/${releaseRepository}/releases/download/${tag}/${candidate.name}` ||
        !releaseAsset ||
        releaseAsset.size !== candidate.size_bytes ||
        releaseAsset.digest !== `sha256:${candidate.sha256}` ||
        requiredChecksum(checksums, candidate.name) !== candidate.sha256
      ) {
        throw new Error(`release-manifest.json does not authenticate ${candidate.name}`);
      }
      supplementalNames.add(candidate.name);
    }
    const artifact = manifest.artifacts.find((candidate) => candidate.target === target);
    if (
      !artifact ||
      artifact.name !== assetName ||
      artifact.path !== assetName ||
      artifact.sha256 !== archiveSha256
    ) {
      throw new Error(`release-manifest.json does not authenticate ${assetName}`);
    }
    exactObject(
      manifest.checksums,
      ["algorithm", "name", "url"],
      "release-manifest.json checksums",
    );
    if (
      manifest.checksums.algorithm !== "sha256" ||
      manifest.checksums.name !== "SHA256SUMS" ||
      manifest.checksums.url !==
        `https://github.com/${releaseRepository}/releases/download/${tag}/SHA256SUMS`
    ) {
      throw new Error("release-manifest.json has an invalid checksum contract");
    }
    exactObject(
      manifest.install,
      ["attestation", "cargo", "github_release", "homebrew_tap", "scoop"],
      "release-manifest.json install",
    );
    exactStringArray(
      manifest.install.attestation,
      [`gh attestation verify 'git-slop-${tag}-<target>.*' --repo ${releaseRepository} --signer-repo ${releaseRepository}`],
      "release-manifest.json install.attestation",
    );
    exactStringArray(
      manifest.install.cargo,
      [`cargo install git-slop --version ${version} --locked`],
      "release-manifest.json install.cargo",
    );
    exactStringArray(
      manifest.install.homebrew_tap,
      ["brew tap coreycoto/tap", "brew install coreycoto/tap/git-slop"],
      "release-manifest.json install.homebrew_tap",
    );
    exactStringArray(
      manifest.install.github_release,
      [
        `gh release download ${tag} --repo ${releaseRepository} --pattern 'git-slop-${tag}-<target>.*' --pattern SHA256SUMS`,
        "sha256sum --check SHA256SUMS --ignore-missing",
      ],
      "release-manifest.json install.github_release",
    );
    exactStringArray(
      manifest.install.scoop,
      [
        "scoop bucket add coreycoto https://github.com/coreycoto/scoop-bucket",
        "scoop install coreycoto/git-slop",
      ],
      "release-manifest.json install.scoop",
    );
    const expectedChecksumNames = new Set([
      ...nameSet,
      ...supplementalNames,
      "release-manifest.json",
    ]);
    if (
      checksums.size !== expectedChecksumNames.size ||
      [...expectedChecksumNames].some((name) => !checksums.has(name))
    ) {
      throw new Error("SHA256SUMS does not contain the exact release asset checksum set");
    }
    const formulaAsset = releaseAssets.get("git-slop.rb");
    if (
      requiredChecksum(checksums, "git-slop.rb") !== formulaSha256 ||
      formulaAsset.digest !== `sha256:${formulaSha256}`
    ) {
      throw new Error("SHA256SUMS does not authenticate git-slop.rb");
    }
    for (const name of sbomAssetNames) {
      const sbomAsset = releaseAssets.get(name);
      if (sbomAsset.digest !== `sha256:${requiredChecksum(checksums, name)}`) {
        throw new Error(`SHA256SUMS does not authenticate ${name}`);
      }
    }
    const manifestAsset = releaseAssets.get("release-manifest.json");
    if (
      requiredChecksum(checksums, "release-manifest.json") !== manifestSha256 ||
      manifestAsset.digest !== `sha256:${manifestSha256}`
    ) {
      throw new Error("SHA256SUMS does not authenticate release-manifest.json");
    }
    return {
      revision,
      crateSha256: crateSource.sha256,
      crateUrl: crateSource.url,
    };
  }


  return {
    exactObject,
    exactReleaseAssets,
    parseChecksums,
    releaseManifestIdentity,
    requiredChecksum,
    verifyReleaseAssetDigest,
  };
}
