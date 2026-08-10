import { createHash } from "node:crypto";
import {
  chmodSync,
  existsSync,
  mkdtempSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { basename, dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

import { createArchiveTools } from "./install/archive.mjs";
import { createBinaryVerifier } from "./install/binary.mjs";
import { createManifestVerifier } from "./install/manifest.mjs";
import { createReleaseApi } from "./install/release-api.mjs";
import { createToolCache } from "./install/tool-cache.mjs";

const releaseVersion = (process.env.GIT_SLOP_ACTION_VERSION || "0.11.3").trim();
const releaseRepository = (
  process.env.GIT_SLOP_RELEASE_REPOSITORY || "coreycoto/git-slop"
).trim();
const githubToken = (process.env.GIT_SLOP_GITHUB_TOKEN || "").trim();
const apiRoot = (process.env.GITHUB_API_URL || "https://api.github.com").replace(/\/$/, "");
const allowDraftRelease =
  (process.env.GIT_SLOP_ALLOW_DRAFT_RELEASE || "false").trim().toLowerCase() === "true" &&
  (process.env.GITHUB_REPOSITORY || "").trim() === releaseRepository;
const releaseIdInput = (process.env.GIT_SLOP_RELEASE_ID || "").trim();
const maximumArchiveBytes = 128 * 1024 * 1024;
const maximumCrateBytes = 16 * 1024 * 1024;
const maximumInventoryBytes = 1024 * 1024;
const maximumReleaseMetadataBytes = 2 * 1024 * 1024;
const maximumManifestBytes = 1024 * 1024;
const maximumFormulaBytes = 1024 * 1024;
const maximumSbomBytes = 4 * 1024 * 1024;
const maximumChecksumBytes = 64 * 1024;
const maximumCrateMetadataBytes = 1024 * 1024;
const maximumTagPeelDepth = 8;
const supportedTargets = {
  "x86_64-unknown-linux-gnu": { os: "linux", arch: "x86_64", archive: "tar.gz" },
  "aarch64-unknown-linux-gnu": { os: "linux", arch: "aarch64", archive: "tar.gz" },
  "aarch64-apple-darwin": { os: "macos", arch: "aarch64", archive: "tar.gz" },
  "x86_64-apple-darwin": { os: "macos", arch: "x86_64", archive: "tar.gz" },
  "x86_64-pc-windows-msvc": { os: "windows", arch: "x86_64", archive: "zip" },
  "aarch64-pc-windows-msvc": { os: "windows", arch: "aarch64", archive: "zip" },
  "x86_64-unknown-linux-musl": { os: "linux", arch: "x86_64", archive: "tar.gz" },
};
const sbomAssetNames = ["git-slop.cdx.json", "git-slop.spdx.json"];
const completionFileNames = ["bash", "zsh", "fish", "powershell", "nushell"].map(
  (shell) => `git-slop.${shell}`,
);
const versionedSchemaName = /^[a-z0-9]+(?:-[a-z0-9]+)*-[1-9][0-9]*\.json$/u;

function appendFileCommand(target, name, value) {
  if (!target) {
    return;
  }
  const digest = createHash("sha256").update(`${name}\0${value}`).digest("hex").slice(0, 24);
  let delimiter = `git_slop_${name}_${digest}`;
  while (String(value).split(/\r?\n/u).includes(delimiter)) delimiter += "_x";
  writeFileSync(target, `${name}<<${delimiter}\n${value}\n${delimiter}\n`, { flag: "a" });
}

function setOutput(name, value) {
  appendFileCommand(process.env.GITHUB_OUTPUT, name, String(value));
}

function fail(message) {
  console.error(`git-slop Action installation failed: ${message}`);
  process.exitCode = 1;
}

function targetTriple() {
  const key = `${process.platform}:${process.arch}`;
  const targets = {
    "linux:x64": "x86_64-unknown-linux-gnu",
    "linux:arm64": "aarch64-unknown-linux-gnu",
    "darwin:arm64": "aarch64-apple-darwin",
    "darwin:x64": "x86_64-apple-darwin",
    "win32:x64": "x86_64-pc-windows-msvc",
    "win32:arm64": "aarch64-pc-windows-msvc",
  };
  const requested = (process.env.GIT_SLOP_TARGET || "").trim();
  const target = requested || targets[key];
  if (!target) {
    throw new Error(`unsupported runner platform ${key}`);
  }
  const metadata = supportedTargets[target];
  const platform = { linux: "linux", darwin: "macos", win32: "windows" }[process.platform];
  const arch = { x64: "x86_64", arm64: "aarch64" }[process.arch] || process.arch;
  if (!metadata || metadata.os !== platform || metadata.arch !== arch) {
    throw new Error(`target ${target} is incompatible with runner platform ${key}`);
  }
  return target;
}


const { downloadAsset, fetchJsonRequired, fetchPublicRequired, readResponseBounded } =
  createReleaseApi({ githubToken });

function sha256(bytes) {
  return createHash("sha256").update(bytes).digest("hex");
}

const {
  exactObject,
  exactReleaseAssets,
  parseChecksums,
  releaseManifestIdentity,
  requiredChecksum,
  verifyReleaseAssetDigest,
} = createManifestVerifier({
  maximumArchiveBytes,
  maximumChecksumBytes,
  maximumFormulaBytes,
  maximumManifestBytes,
  maximumSbomBytes,
  sbomAssetNames,
  sha256,
  supportedTargets,
});

const {
  archiveTarExecutable,
  exactTagRevision,
  materializeArchive,
  validateArchiveFormat,
  verifyCanonicalCrate,
} = createArchiveTools({
  apiRoot,
  completionFileNames,
  fetchJsonRequired,
  maximumArchiveBytes,
  maximumReleaseMetadataBytes,
  maximumTagPeelDepth,
  maximumInventoryBytes,
  maximumCrateMetadataBytes,
  sha256,
  versionedSchemaName,
});

const { verifyInstalledBuildInfo, verifyInstalledVersion } = createBinaryVerifier({
  exactObject,
});

const { cachedBinary, populateToolCache, toolCacheDirectory } = createToolCache({
  exactObject,
  maximumArchiveBytes,
  sha256,
  verifyInstalledBuildInfo,
  verifyInstalledVersion,
});

function publishInstallOutputs({
  version,
  target,
  assetName,
  assetUrl,
  binaryPath,
  archiveSha256,
  revision,
  crateSha256,
  manifestSha256,
  cacheHit,
}) {
  if (process.env.GITHUB_PATH) {
    writeFileSync(process.env.GITHUB_PATH, `${dirname(binaryPath)}\n`, { flag: "a" });
  }
  setOutput("version", version);
  setOutput("target", target);
  setOutput("asset", assetName);
  setOutput("asset-url", assetUrl);
  setOutput("binary-path", binaryPath);
  setOutput("sha256", archiveSha256);
  setOutput("source-revision", revision);
  setOutput("crate-sha256", crateSha256);
  setOutput("release-manifest-sha256", manifestSha256);
  setOutput("cache-hit", cacheHit);
}

async function main() {
  const version = releaseVersion.startsWith("v") ? releaseVersion.slice(1) : releaseVersion;
  if (!/^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$/u.test(version)) {
    throw new Error(`invalid release version ${JSON.stringify(releaseVersion)}`);
  }
  if (!/^[A-Za-z0-9_.-]+\/[A-Za-z0-9_.-]+$/u.test(releaseRepository)) {
    throw new Error(`invalid release repository ${JSON.stringify(releaseRepository)}`);
  }

  const tag = `v${version}`;
  const target = targetTriple();
  const targetMetadata = supportedTargets[target];
  const extension = process.platform === "win32" ? "zip" : "tar.gz";
  if (extension !== targetMetadata.archive) {
    throw new Error(`runner archive format does not match target ${target}`);
  }
  const assetName = `git-slop-${tag}-${target}.${extension}`;
  let expectedReleaseId = null;
  let releaseUrl;
  if (releaseIdInput) {
    if (!allowDraftRelease || !githubToken) {
      throw new Error(
        "release ID lookup is restricted to authenticated same-repository draft verification",
      );
    }
    if (!/^[1-9]\d*$/u.test(releaseIdInput)) {
      throw new Error("release ID must be a positive decimal integer");
    }
    expectedReleaseId = Number(releaseIdInput);
    if (!Number.isSafeInteger(expectedReleaseId)) {
      throw new Error("release ID exceeds the safe integer range");
    }
    releaseUrl = `${apiRoot}/repos/${releaseRepository}/releases/${releaseIdInput}`;
  } else {
    if (allowDraftRelease) {
      throw new Error("same-repository draft verification requires an exact release ID");
    }
    releaseUrl = `${apiRoot}/repos/${releaseRepository}/releases/tags/${encodeURIComponent(tag)}`;
  }
  const release = await fetchJsonRequired(
    releaseUrl,
    maximumReleaseMetadataBytes,
    `release metadata for ${tag}`,
  );
  if (
    release === null ||
    typeof release !== "object" ||
    Array.isArray(release) ||
    (expectedReleaseId !== null && release.id !== expectedReleaseId) ||
    release.tag_name !== tag ||
    release.prerelease !== false ||
    typeof release.draft !== "boolean" ||
    (release.draft === true
      ? release.published_at !== null
      : typeof release.published_at !== "string")
  ) {
    throw new Error(`release ${tag} has inconsistent GitHub release metadata`);
  }
  if (release.draft === true && !allowDraftRelease) {
    throw new Error(`release ${tag} is still a draft`);
  }
  const releaseAssets = exactReleaseAssets(release, releaseRepository, tag);
  const archiveAsset = releaseAssets.get(assetName);
  const checksumAsset = releaseAssets.get("SHA256SUMS");
  const manifestAsset = releaseAssets.get("release-manifest.json");
  const formulaAsset = releaseAssets.get("git-slop.rb");

  console.log(`Verifying ${releaseRepository} ${assetName}`);
  const [checksumBytes, manifestBytes, formulaBytes] = await Promise.all([
    downloadAsset(checksumAsset, maximumChecksumBytes),
    downloadAsset(manifestAsset, maximumManifestBytes),
    downloadAsset(formulaAsset, maximumFormulaBytes),
  ]);
  verifyReleaseAssetDigest(checksumAsset, checksumBytes);
  const manifestSha256 = verifyReleaseAssetDigest(manifestAsset, manifestBytes);
  const formulaSha256 = verifyReleaseAssetDigest(formulaAsset, formulaBytes);
  const checksums = parseChecksums(checksumBytes.toString("utf8"));
  const expected = requiredChecksum(checksums, assetName);
  const releaseArchiveSha256 = String(archiveAsset.digest || "").replace(/^sha256:/u, "");
  if (releaseArchiveSha256 !== expected) {
    throw new Error(
      `release metadata SHA-256 mismatch for ${assetName}: checksums=${expected}, GitHub=${releaseArchiveSha256}`,
    );
  }
  const identity = releaseManifestIdentity(manifestBytes, {
    version,
    tag,
    target,
    assetName,
    releaseRepository,
    archiveSha256: releaseArchiveSha256,
    checksums,
    releaseAssets,
    manifestSha256,
    formulaSha256,
  });
  const taggedRevision = await exactTagRevision(releaseRepository, tag);
  if (taggedRevision !== identity.revision) {
    throw new Error(
      `release tag ${tag} resolves to ${taggedRevision}, expected ${identity.revision}`,
    );
  }

  const executableName = targetMetadata.os === "windows" ? "git-slop.exe" : "git-slop";
  const cacheDirectory = toolCacheDirectory(version, target, identity.revision, manifestSha256);
  const cacheExpected = {
    schema_version: 1,
    version,
    target,
    revision: identity.revision,
    manifest_sha256: manifestSha256,
    archive_sha256: releaseArchiveSha256,
  };
  const cached = cachedBinary(cacheDirectory, executableName, cacheExpected);
  if (cached) {
    publishInstallOutputs({
      version,
      target,
      assetName,
      assetUrl: archiveAsset.browser_download_url,
      binaryPath: cached.binaryPath,
      archiveSha256: releaseArchiveSha256,
      revision: identity.revision,
      crateSha256: cached.crateSha256,
      manifestSha256,
      cacheHit: true,
    });
    console.log(`Verified cached ${assetName} (${releaseArchiveSha256})`);
    return;
  }

  console.log(`Downloading ${releaseRepository} ${assetName}`);
  const archiveBytes = await downloadAsset(archiveAsset, maximumArchiveBytes);
  const actual = verifyReleaseAssetDigest(archiveAsset, archiveBytes);
  if (actual !== releaseArchiveSha256) {
    throw new Error(`SHA-256 mismatch for ${assetName}: expected ${releaseArchiveSha256}, received ${actual}`);
  }
  validateArchiveFormat(archiveBytes, extension);

  const baseTemp = process.env.RUNNER_TEMP || tmpdir();
  const installRoot = mkdtempSync(join(baseTemp, "git-slop-action-"));
  const archivePath = join(installRoot, basename(assetName));
  writeFileSync(archivePath, archiveBytes, { mode: 0o600 });
  const crateResponse = await fetchPublicRequired(identity.crateUrl);
  const crateBytes = await readResponseBounded(
    crateResponse,
    maximumCrateBytes,
    `canonical crates.io package git-slop-${version}.crate`,
  );
  const cratePath = join(installRoot, `git-slop-${version}.crate`);
  writeFileSync(cratePath, crateBytes, { mode: 0o600 });
  const crateSha256 = verifyCanonicalCrate(
    cratePath,
    crateBytes,
    version,
    identity.crateSha256,
    identity.revision,
  );

  const rootName = assetName.slice(0, -(`.${extension}`.length));
  let binaryPath = materializeArchive(
    archivePath,
    installRoot,
    rootName,
    executableName,
  );
  if (!binaryPath || !existsSync(binaryPath)) {
    throw new Error(`${assetName} did not contain ${executableName}`);
  }
  if (process.platform !== "win32") {
    chmodSync(binaryPath, 0o755);
  }
  verifyInstalledVersion(binaryPath, version);
  verifyInstalledBuildInfo(binaryPath, version, identity.revision);
  binaryPath = populateToolCache(cacheDirectory, executableName, binaryPath, {
    ...cacheExpected,
    crate_sha256: crateSha256,
  });
  if (cacheDirectory) {
    const stored = cachedBinary(cacheDirectory, executableName, cacheExpected);
    if (!stored || stored.crateSha256 !== crateSha256) {
      throw new Error("newly populated Git Slop tool cache failed verification");
    }
    binaryPath = stored.binaryPath;
  }
  publishInstallOutputs({
    version,
    target,
    assetName,
    assetUrl: archiveAsset.browser_download_url,
    binaryPath,
    archiveSha256: actual,
    revision: identity.revision,
    crateSha256,
    manifestSha256,
    cacheHit: false,
  });
  console.log(`Verified ${assetName} (${actual})`);
}

export {
  archiveTarExecutable,
  exactTagRevision,
  main,
  materializeArchive,
  validateArchiveFormat,
  verifyCanonicalCrate,
};

if (process.argv[1] && fileURLToPath(import.meta.url) === resolve(process.argv[1])) {
  main().catch((error) => fail(error instanceof Error ? error.message : String(error)));
}
