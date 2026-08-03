import { createHash } from "node:crypto";
import { chmodSync, existsSync, mkdtempSync, readdirSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { basename, dirname, join } from "node:path";
import { spawnSync } from "node:child_process";

const releaseVersion = (process.env.GIT_SLOP_ACTION_VERSION || "0.9.0").trim();
const releaseRepository = (
  process.env.GIT_SLOP_RELEASE_REPOSITORY || "coreycoto/git-slop"
).trim();
const githubToken = (process.env.GIT_SLOP_GITHUB_TOKEN || "").trim();
const apiRoot = (process.env.GITHUB_API_URL || "https://api.github.com").replace(/\/$/, "");

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

async function downloadAsset(asset) {
  const response = await fetchRequired(asset.url, "application/octet-stream");
  return Buffer.from(await response.arrayBuffer());
}

function expectedChecksum(manifest, assetName) {
  for (const line of manifest.split(/\r?\n/u)) {
    const match = line.match(/^([a-f0-9]{64}) {2}(.+)$/u);
    if (match && match[2] === assetName) {
      return match[1];
    }
  }
  throw new Error(`SHA256SUMS has no exact entry for ${assetName}`);
}

function findExecutable(root, executableName) {
  const pending = [root];
  while (pending.length > 0) {
    const current = pending.shift();
    for (const entry of readdirSync(current, { withFileTypes: true })) {
      const candidate = join(current, entry.name);
      if (entry.isDirectory()) {
        pending.push(candidate);
      } else if (entry.isFile() && entry.name === executableName) {
        return candidate;
      }
    }
  }
  return null;
}

function extractArchive(archivePath, installRoot) {
  const result = spawnSync("tar", ["-xf", archivePath, "-C", installRoot], {
    encoding: "utf8",
    stdio: ["ignore", "pipe", "pipe"],
  });
  if (result.status !== 0) {
    throw new Error(`archive extraction failed: ${(result.stderr || "").trim()}`);
  }
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

async function main() {
  const version = releaseVersion.startsWith("v") ? releaseVersion.slice(1) : releaseVersion;
  if (!/^\d+\.\d+\.\d+(?:[-+][0-9A-Za-z.-]+)?$/u.test(version)) {
    throw new Error(`invalid release version ${JSON.stringify(releaseVersion)}`);
  }
  if (!/^[A-Za-z0-9_.-]+\/[A-Za-z0-9_.-]+$/u.test(releaseRepository)) {
    throw new Error(`invalid release repository ${JSON.stringify(releaseRepository)}`);
  }

  const tag = `v${version}`;
  const target = targetTriple();
  const extension = process.platform === "win32" ? "zip" : "tar.gz";
  const assetName = `git-slop-${tag}-${target}.${extension}`;
  const releaseUrl = `${apiRoot}/repos/${releaseRepository}/releases/tags/${encodeURIComponent(tag)}`;
  const releaseResponse = await fetchRequired(releaseUrl);
  const release = await releaseResponse.json();
  const archiveAsset = release.assets?.find((asset) => asset.name === assetName);
  const checksumAsset = release.assets?.find((asset) => asset.name === "SHA256SUMS");
  if (!archiveAsset) {
    throw new Error(`release ${tag} has no asset named ${assetName}`);
  }
  if (!checksumAsset) {
    throw new Error(`release ${tag} has no SHA256SUMS asset`);
  }

  console.log(`Downloading ${releaseRepository} ${assetName}`);
  const [archiveBytes, checksumBytes] = await Promise.all([
    downloadAsset(archiveAsset),
    downloadAsset(checksumAsset),
  ]);
  if (archiveAsset.size && archiveBytes.length !== archiveAsset.size) {
    throw new Error(
      `${assetName} size mismatch: expected ${archiveAsset.size}, received ${archiveBytes.length}`,
    );
  }
  const expected = expectedChecksum(checksumBytes.toString("utf8"), assetName);
  const actual = createHash("sha256").update(archiveBytes).digest("hex");
  if (actual !== expected) {
    throw new Error(`SHA-256 mismatch for ${assetName}: expected ${expected}, received ${actual}`);
  }

  const baseTemp = process.env.RUNNER_TEMP || tmpdir();
  const installRoot = mkdtempSync(join(baseTemp, "git-slop-action-"));
  const archivePath = join(installRoot, basename(assetName));
  writeFileSync(archivePath, archiveBytes, { mode: 0o600 });
  extractArchive(archivePath, installRoot);

  const executableName = process.platform === "win32" ? "git-slop.exe" : "git-slop";
  const binaryPath = findExecutable(installRoot, executableName);
  if (!binaryPath || !existsSync(binaryPath)) {
    throw new Error(`${assetName} did not contain ${executableName}`);
  }
  if (process.platform !== "win32") {
    chmodSync(binaryPath, 0o755);
  }
  verifyInstalledVersion(binaryPath, version);

  if (process.env.GITHUB_PATH) {
    writeFileSync(process.env.GITHUB_PATH, `${dirname(binaryPath)}\n`, { flag: "a" });
  }
  setOutput("version", version);
  setOutput("target", target);
  setOutput("asset", assetName);
  setOutput("asset-url", archiveAsset.browser_download_url);
  setOutput("binary-path", binaryPath);
  setOutput("sha256", actual);
  console.log(`Verified ${assetName} (${actual})`);
}

main().catch((error) => fail(error instanceof Error ? error.message : String(error)));
