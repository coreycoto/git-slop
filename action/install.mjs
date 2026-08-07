import { createHash } from "node:crypto";
import { chmodSync, existsSync, mkdirSync, mkdtempSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { basename, dirname, isAbsolute, join, resolve } from "node:path";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";

const releaseVersion = (process.env.GIT_SLOP_ACTION_VERSION || "0.9.5").trim();
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
const maximumChecksumBytes = 64 * 1024;
const maximumCrateMetadataBytes = 1024 * 1024;
const maximumTagPeelDepth = 8;
const supportedTargets = {
  "x86_64-unknown-linux-gnu": { os: "linux", arch: "x86_64", archive: "tar.gz" },
  "aarch64-unknown-linux-gnu": { os: "linux", arch: "aarch64", archive: "tar.gz" },
  "aarch64-apple-darwin": { os: "macos", arch: "aarch64", archive: "tar.gz" },
  "x86_64-pc-windows-msvc": { os: "windows", arch: "x86_64", archive: "zip" },
  "aarch64-pc-windows-msvc": { os: "windows", arch: "aarch64", archive: "zip" },
};

function appendFileCommand(target, name, value) {
  if (!target) {
    return;
  }
  const delimiter = `git_slop_${name}_${Date.now()}_${Math.random().toString(16).slice(2)}`;
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
    "win32:x64": "x86_64-pc-windows-msvc",
    "win32:arm64": "aarch64-pc-windows-msvc",
  };
  const target = targets[key];
  if (!target) {
    throw new Error(`unsupported runner platform ${key}`);
  }
  return target;
}

function requestHeaders(accept = "application/vnd.github+json") {
  const headers = {
    Accept: accept,
    "User-Agent": "coreycoto-git-slop-action",
    "X-GitHub-Api-Version": "2022-11-28",
  };
  if (githubToken) {
    headers.Authorization = `Bearer ${githubToken}`;
  }
  return headers;
}

async function fetchRequired(url, accept) {
  const response = await fetch(url, {
    headers: requestHeaders(accept),
    redirect: "follow",
  });
  if (!response.ok) {
    throw new Error(`${url} returned HTTP ${response.status}`);
  }
  return response;
}

async function fetchPublicRequired(url, accept = "application/octet-stream") {
  const response = await fetch(url, {
    headers: {
      Accept: accept,
      "User-Agent": "coreycoto-git-slop-action",
    },
    redirect: "follow",
  });
  if (!response.ok) {
    throw new Error(`${url} returned HTTP ${response.status}`);
  }
  return response;
}

async function readResponseBounded(response, maximumBytes, label, expectedBytes = null) {
  const declaredLength = response.headers.get("content-length");
  if (declaredLength !== null) {
    if (!/^\d+$/u.test(declaredLength)) {
      throw new Error(`${label} returned an invalid Content-Length`);
    }
    const parsedLength = Number(declaredLength);
    if (!Number.isSafeInteger(parsedLength) || parsedLength > maximumBytes) {
      throw new Error(`${label} exceeds its download size limit`);
    }
    if (expectedBytes !== null && parsedLength !== expectedBytes) {
      throw new Error(
        `${label} Content-Length mismatch: expected ${expectedBytes}, received ${parsedLength}`,
      );
    }
  }
  if (!response.body) {
    throw new Error(`${label} returned no response body`);
  }
  const reader = response.body.getReader();
  const chunks = [];
  let size = 0;
  while (true) {
    const { done, value } = await reader.read();
    if (done) {
      break;
    }
    size += value.byteLength;
    if (size > maximumBytes || (expectedBytes !== null && size > expectedBytes)) {
      await reader.cancel();
      throw new Error(`${label} exceeds its download size limit`);
    }
    chunks.push(Buffer.from(value));
  }
  if (expectedBytes !== null && size !== expectedBytes) {
    throw new Error(`${label} size mismatch: expected ${expectedBytes}, received ${size}`);
  }
  return Buffer.concat(chunks, size);
}

async function fetchJsonRequired(url, maximumBytes, label) {
  const response = await fetchRequired(url);
  const bytes = await readResponseBounded(response, maximumBytes, label);
  try {
    return JSON.parse(bytes.toString("utf8"));
  } catch (error) {
    throw new Error(`${label} returned invalid JSON: ${error.message}`);
  }
}

async function downloadAsset(asset, maximumBytes) {
  const response = await fetchRequired(asset.url, "application/octet-stream");
  return readResponseBounded(response, maximumBytes, `release asset ${asset.name}`, asset.size);
}

function sha256(bytes) {
  return createHash("sha256").update(bytes).digest("hex");
}

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
    "release-manifest.json",
  ]);
  if (!Array.isArray(release.assets) || release.assets.length !== expectedNames.size) {
    throw new Error(`release ${tag} must contain exactly eight distribution assets`);
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

function validateArchiveFormat(bytes, archive) {
  if (!Buffer.isBuffer(bytes) || bytes.length < 4) {
    throw new Error(`release archive is too small to be a valid ${archive} file`);
  }
  if (archive === "tar.gz") {
    if (bytes[0] !== 0x1f || bytes[1] !== 0x8b || bytes[2] !== 0x08) {
      throw new Error("release archive does not use the required tar.gz format");
    }
    return;
  }
  if (archive === "zip") {
    if (bytes[0] !== 0x50 || bytes[1] !== 0x4b || bytes[2] !== 0x03 || bytes[3] !== 0x04) {
      throw new Error("release archive does not use the required ZIP format");
    }
    return;
  }
  throw new Error(`unsupported release archive format ${archive}`);
}

function archiveTarExecutable() {
  if (process.platform !== "win32") {
    return "tar";
  }

  const systemRoot =
    (process.env.SystemRoot || "").trim() || (process.env.WINDIR || "").trim();
  if (!systemRoot || !isAbsolute(systemRoot)) {
    throw new Error("Windows SystemRoot must be an absolute path");
  }
  const executable = join(systemRoot, "System32", "tar.exe");
  if (!existsSync(executable)) {
    throw new Error(`Windows built-in tar.exe is unavailable at ${executable}`);
  }
  return executable;
}

function runTar(args, options = {}) {
  const result = spawnSync(archiveTarExecutable(), args, {
    cwd: options.cwd,
    encoding: Object.hasOwn(options, "encoding") ? options.encoding : "utf8",
    maxBuffer: options.maxBuffer ?? maximumInventoryBytes,
    stdio: ["ignore", "pipe", "pipe"],
  });
  if (result.error) {
    throw new Error(`archive inspection failed: ${result.error.message}`);
  }
  if (result.status !== 0) {
    const stderr = Buffer.isBuffer(result.stderr)
      ? result.stderr.toString("utf8")
      : result.stderr || "";
    throw new Error(`archive inspection failed: ${stderr.trim()}`);
  }
  return result.stdout;
}

function runArchiveTar(archivePath, operationArguments, memberArguments = [], options = {}) {
  const absoluteArchivePath = resolve(archivePath);
  return runTar(
    [...operationArguments, "-f", basename(absoluteArchivePath), ...memberArguments],
    {
      ...options,
      cwd: dirname(absoluteArchivePath),
    },
  );
}

function validateArchiveMember(member, rootName) {
  if (
    !member ||
    member.includes("\\") ||
    member.startsWith("/") ||
    member.startsWith("~") ||
    /^[A-Za-z]:/u.test(member)
  ) {
    throw new Error(`archive contains an unsafe member name: ${JSON.stringify(member)}`);
  }
  const components = member.replace(/\/$/u, "").split("/");
  if (components.some((component) => component === "" || component === "." || component === "..")) {
    throw new Error(`archive contains an unsafe member name: ${JSON.stringify(member)}`);
  }
  if (components[0] !== rootName) {
    throw new Error(`archive member escapes the expected root ${rootName}: ${member}`);
  }
}

function validateCrateInventory(archivePath, rootName) {
  const inventoryText = runArchiveTar(archivePath, ["-t"]);
  const inventory = inventoryText.split(/\r?\n/u).filter(Boolean);
  if (inventory.length === 0 || inventory.length > 4096) {
    throw new Error("canonical crates.io package has an invalid archive inventory size");
  }
  const seen = new Set();
  for (const member of inventory) {
    validateArchiveMember(member, rootName);
    if (seen.has(member)) {
      throw new Error(`canonical crates.io package contains a duplicate member: ${member}`);
    }
    seen.add(member);
  }
  for (const member of [
    `${rootName}/Cargo.toml`,
    `${rootName}/.cargo_vcs_info.json`,
    `${rootName}/src/main.rs`,
  ]) {
    if (!seen.has(member)) {
      throw new Error(`canonical crates.io package is missing required member ${member}`);
    }
  }
}

function verifyCrateVcsMetadata(archivePath, rootName, revision) {
  const member = `${rootName}/.cargo_vcs_info.json`;
  const bytes = runArchiveTar(
    archivePath,
    ["-xO"],
    [member],
    {
      encoding: null,
      maxBuffer: maximumCrateMetadataBytes,
    },
  );
  if (
    !Buffer.isBuffer(bytes) ||
    bytes.length === 0 ||
    bytes.length > maximumCrateMetadataBytes
  ) {
    throw new Error("canonical crates.io package has invalid VCS metadata size");
  }
  let metadata;
  try {
    metadata = JSON.parse(bytes.toString("utf8"));
  } catch (error) {
    throw new Error(`canonical crates.io package has invalid VCS metadata: ${error.message}`);
  }
  if (
    metadata === null ||
    typeof metadata !== "object" ||
    Array.isArray(metadata) ||
    metadata.git === null ||
    typeof metadata.git !== "object" ||
    Array.isArray(metadata.git) ||
    metadata.git.sha1 !== revision ||
    (metadata.git.dirty !== undefined && metadata.git.dirty !== false) ||
    metadata.path_in_vcs !== ""
  ) {
    throw new Error(
      "canonical crates.io package VCS metadata does not match the clean release revision",
    );
  }
}

function verifyCanonicalCrate(cratePath, crateBytes, version, expectedSha256, revision) {
  validateArchiveFormat(crateBytes, "tar.gz");
  const actualSha256 = sha256(crateBytes);
  if (actualSha256 !== expectedSha256) {
    throw new Error(
      `canonical crates.io package SHA-256 mismatch: expected ${expectedSha256}, received ${actualSha256}`,
    );
  }
  const rootName = `git-slop-${version}`;
  validateCrateInventory(cratePath, rootName);
  verifyCrateVcsMetadata(cratePath, rootName, revision);
  return actualSha256;
}

async function exactTagRevision(repository, tag) {
  const encodedTag = encodeURIComponent(tag);
  const reference = await fetchJsonRequired(
    `${apiRoot}/repos/${repository}/git/ref/tags/${encodedTag}`,
    maximumReleaseMetadataBytes,
    `release tag reference for ${tag}`,
  );
  if (
    reference === null ||
    typeof reference !== "object" ||
    Array.isArray(reference) ||
    reference.ref !== `refs/tags/${tag}` ||
    reference.object === null ||
    typeof reference.object !== "object" ||
    Array.isArray(reference.object) ||
    !["commit", "tag"].includes(reference.object.type) ||
    !/^[a-f0-9]{40}$/u.test(reference.object.sha || "")
  ) {
    throw new Error(`release tag ${tag} has invalid exact-reference metadata`);
  }

  let objectType = reference.object.type;
  let objectSha = reference.object.sha;
  const seen = new Set();
  for (let depth = 0; depth <= maximumTagPeelDepth; depth += 1) {
    if (objectType === "commit") {
      return objectSha;
    }
    if (depth === maximumTagPeelDepth || seen.has(objectSha)) {
      throw new Error(`release tag ${tag} exceeds the safe annotated-tag peel limit`);
    }
    seen.add(objectSha);
    const annotated = await fetchJsonRequired(
      `${apiRoot}/repos/${repository}/git/tags/${objectSha}`,
      maximumReleaseMetadataBytes,
      `annotated release tag object ${objectSha}`,
    );
    if (
      annotated === null ||
      typeof annotated !== "object" ||
      Array.isArray(annotated) ||
      annotated.sha !== objectSha ||
      (depth === 0 && annotated.tag !== tag) ||
      annotated.object === null ||
      typeof annotated.object !== "object" ||
      Array.isArray(annotated.object) ||
      !["commit", "tag"].includes(annotated.object.type) ||
      !/^[a-f0-9]{40}$/u.test(annotated.object.sha || "")
    ) {
      throw new Error(`release tag ${tag} has invalid annotated-tag metadata`);
    }
    objectType = annotated.object.type;
    objectSha = annotated.object.sha;
  }
  throw new Error(`release tag ${tag} could not be resolved to a commit`);
}

function materializeArchive(archivePath, installRoot, rootName, executableName) {
  const expectedFiles = ["LICENSE", "README.md", "git-slop.1", executableName];
  const rootMember = `${rootName}/`;
  const expectedFileMembers = expectedFiles.map((name) => `${rootName}/${name}`);
  const allowedMembers = new Set([rootMember, ...expectedFileMembers]);
  const inventoryText = runArchiveTar(archivePath, ["-t"]);
  const inventory = inventoryText.split(/\r?\n/u).filter(Boolean);
  const actualMembers = new Set();
  for (const member of inventory) {
    validateArchiveMember(member, rootName);
    if (actualMembers.has(member)) {
      throw new Error(`archive contains a duplicate member: ${member}`);
    }
    if (!allowedMembers.has(member)) {
      throw new Error("archive inventory does not match the exact Git Slop release layout");
    }
    actualMembers.add(member);
  }
  if (expectedFileMembers.some((member) => !actualMembers.has(member))) {
    throw new Error("archive inventory does not match the exact Git Slop release layout");
  }

  const payloadRoot = join(installRoot, "payload");
  mkdirSync(payloadRoot, { mode: 0o700 });
  for (const name of expectedFiles) {
    const member = `${rootName}/${name}`;
    const maximumBytes = name === executableName ? maximumArchiveBytes : 4 * 1024 * 1024;
    const bytes = runArchiveTar(
      archivePath,
      ["-xO"],
      [member],
      {
        encoding: null,
        maxBuffer: maximumBytes,
      },
    );
    if (!Buffer.isBuffer(bytes) || bytes.length === 0 || bytes.length > maximumBytes) {
      throw new Error(`archive member ${member} has an invalid size`);
    }
    writeFileSync(join(payloadRoot, name), bytes, {
      mode: name === executableName ? 0o700 : 0o600,
    });
  }
  return join(payloadRoot, executableName);
}

function verifyInstalledVersion(binaryPath, version) {
  const result = spawnSync(binaryPath, ["version"], {
    encoding: "utf8",
    stdio: ["ignore", "pipe", "pipe"],
  });
  if (result.status !== 0) {
    throw new Error(`installed binary failed its version smoke test: ${(result.stderr || "").trim()}`);
  }
  const output = (result.stdout || "").trim();
  if (output !== `git-slop ${version}`) {
    throw new Error(`installed binary reported an unexpected version: ${output}`);
  }
}

function verifyInstalledBuildInfo(binaryPath, version, revision) {
  const result = spawnSync(binaryPath, ["build-info", "--format", "json"], {
    encoding: "utf8",
    stdio: ["ignore", "pipe", "pipe"],
  });
  if (result.status !== 0) {
    throw new Error(
      `installed binary failed its build-info smoke test: ${(result.stderr || "").trim()}`,
    );
  }
  let buildInfo;
  try {
    buildInfo = JSON.parse(result.stdout || "");
  } catch (error) {
    throw new Error(`installed binary emitted invalid build-info JSON: ${error.message}`);
  }
  exactObject(
    buildInfo,
    ["project", "schema_version", "source_dirty", "source_revision", "version"],
    "installed binary build-info",
  );
  if (
    buildInfo.schema_version !== 1 ||
    buildInfo.project !== "git-slop" ||
    buildInfo.version !== version ||
    buildInfo.source_revision !== revision ||
    buildInfo.source_dirty !== false
  ) {
    throw new Error("installed binary build identity does not match the verified release manifest");
  }
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
  if (!Array.isArray(manifest.artifacts) || manifest.artifacts.length !== 5) {
    throw new Error("release-manifest.json must describe exactly five release targets");
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
    ["github_release", "homebrew_tap"],
    "release-manifest.json install",
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
  const expectedChecksumNames = new Set([
    ...nameSet,
    "git-slop.rb",
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

  console.log(`Downloading ${releaseRepository} ${assetName}`);
  const [archiveBytes, checksumBytes, manifestBytes, formulaBytes] = await Promise.all([
    downloadAsset(archiveAsset, maximumArchiveBytes),
    downloadAsset(checksumAsset, maximumChecksumBytes),
    downloadAsset(manifestAsset, maximumManifestBytes),
    downloadAsset(formulaAsset, maximumFormulaBytes),
  ]);
  const actual = verifyReleaseAssetDigest(archiveAsset, archiveBytes);
  verifyReleaseAssetDigest(checksumAsset, checksumBytes);
  const manifestSha256 = verifyReleaseAssetDigest(manifestAsset, manifestBytes);
  const formulaSha256 = verifyReleaseAssetDigest(formulaAsset, formulaBytes);
  validateArchiveFormat(archiveBytes, extension);
  const checksums = parseChecksums(checksumBytes.toString("utf8"));
  const expected = requiredChecksum(checksums, assetName);
  if (actual !== expected) {
    throw new Error(`SHA-256 mismatch for ${assetName}: expected ${expected}, received ${actual}`);
  }
  const identity = releaseManifestIdentity(manifestBytes, {
    version,
    tag,
    target,
    assetName,
    releaseRepository,
    archiveSha256: actual,
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

  const executableName = targetMetadata.os === "windows" ? "git-slop.exe" : "git-slop";
  const rootName = assetName.slice(0, -(`.${extension}`.length));
  const binaryPath = materializeArchive(
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

  if (process.env.GITHUB_PATH) {
    writeFileSync(process.env.GITHUB_PATH, `${dirname(binaryPath)}\n`, { flag: "a" });
  }
  setOutput("version", version);
  setOutput("target", target);
  setOutput("asset", assetName);
  setOutput("asset-url", archiveAsset.browser_download_url);
  setOutput("binary-path", binaryPath);
  setOutput("sha256", actual);
  setOutput("source-revision", identity.revision);
  setOutput("crate-sha256", crateSha256);
  setOutput("release-manifest-sha256", manifestSha256);
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
