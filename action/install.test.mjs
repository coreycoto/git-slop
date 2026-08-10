import assert from "node:assert/strict";
import { spawn, spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import {
  chmodSync,
  existsSync,
  mkdtempSync,
  mkdirSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { createServer } from "node:http";
import { tmpdir } from "node:os";
import { basename, dirname, isAbsolute, join, resolve } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import test from "node:test";

import {
  archiveTarExecutable,
  materializeArchive,
  validateArchiveFormat,
  verifyCanonicalCrate,
} from "./install.mjs";
import { isolatedActionEnvironment } from "./test-environment.mjs";
import { exerciseArchiveLayout } from "./install/test-scenarios/archive-layout.mjs";
import { exerciseCrateSource } from "./install/test-scenarios/crate-source.mjs";
import { exerciseDraftRelease } from "./install/test-scenarios/draft-release.mjs";
import { exerciseInstallAndCache } from "./install/test-scenarios/install-and-cache.mjs";
import { exerciseReleaseAssets } from "./install/test-scenarios/release-assets.mjs";
import { exerciseReleaseManifest } from "./install/test-scenarios/release-manifest.mjs";
import { exerciseTagIdentity } from "./install/test-scenarios/tag-identity.mjs";

const actionDirectory = dirname(fileURLToPath(import.meta.url));
const installer = join(actionDirectory, "install.mjs");
const releaseManifestFixture = JSON.parse(
  readFileSync(
    join(actionDirectory, "..", "xtask", "tests", "fixtures", "release-manifest-v0.9.0.json"),
    "utf8",
  ),
);

function targetTriple() {
  const target = {
    "linux:x64": "x86_64-unknown-linux-gnu",
    "linux:arm64": "aarch64-unknown-linux-gnu",
    "darwin:arm64": "aarch64-apple-darwin",
  }[`${process.platform}:${process.arch}`];
  if (!target) {
    throw new Error(`test does not support ${process.platform}:${process.arch}`);
  }
  return target;
}

function outputs(path) {
  const lines = readFileSync(path, "utf8").split(/\r?\n/u);
  const result = {};
  for (let index = 0; index < lines.length; index += 1) {
    const match = lines[index].match(/^([^<]+)<<(.+)$/u);
    if (!match) {
      continue;
    }
    const [, name, delimiter] = match;
    const values = [];
    index += 1;
    while (index < lines.length && lines[index] !== delimiter) {
      values.push(lines[index]);
      index += 1;
    }
    result[name] = values.join("\n");
  }
  return result;
}

function runNode(script, environment) {
  return new Promise((resolve) => {
    const child = spawn(process.execPath, [script], {
      env: isolatedActionEnvironment(environment),
      stdio: ["ignore", "pipe", "pipe"],
    });
    let stdout = "";
    let stderr = "";
    child.stdout.setEncoding("utf8");
    child.stderr.setEncoding("utf8");
    child.stdout.on("data", (chunk) => {
      stdout += chunk;
    });
    child.stderr.on("data", (chunk) => {
      stderr += chunk;
    });
    child.on("close", (status) => resolve({ status, stdout, stderr }));
  });
}

function runArchiveTar(archivePath, operationArguments, trailingArguments = [], options = {}) {
  const absoluteArchivePath = resolve(archivePath);
  return spawnSync(
    archiveTarExecutable(),
    [...operationArguments, "-f", basename(absoluteArchivePath), ...trailingArguments],
    {
      ...options,
      cwd: dirname(absoluteArchivePath),
    },
  );
}

function createArchive(archivePath, operationArguments, trailingArguments) {
  const result = runArchiveTar(archivePath, operationArguments, trailingArguments, {
    encoding: "utf8",
  });
  assert.equal(result.status, 0, result.error?.message || result.stderr);
}

test(
  "installer downloads the exact target archive and verifies its checksum and version",
  {
    skip:
      process.platform === "win32" ||
      (process.platform === "darwin" && process.arch === "x64"),
  },
  async () => {
    const root = mkdtempSync(join(tmpdir(), "git-slop-installer-test-"));
    const version = "0.9.0";
    const tag = `v${version}`;
    const revision = "a".repeat(40);
    const target = targetTriple();
    const assetName = `git-slop-${tag}-${target}.tar.gz`;
    const stageName = `git-slop-${tag}-${target}`;
    const stage = join(root, "stage", stageName);
    const archive = join(root, assetName);
    const output = join(root, "github-output.txt");
    const githubPath = join(root, "github-path.txt");
    mkdirSync(stage, { recursive: true });
    const binary = join(stage, "git-slop");
    writeFileSync(
      binary,
      `#!/usr/bin/env node
if (process.argv[2] === "version") {
  process.stdout.write("git-slop ${version}\\n");
} else if (process.argv[2] === "build-info" && process.argv[3] === "--format" && process.argv[4] === "json") {
  process.stdout.write(JSON.stringify({schema_version: 1, project: "git-slop", version: "${version}", source_revision: "${revision}", source_dirty: false}) + "\\n");
} else {
  process.exitCode = 2;
}
`,
      "utf8",
    );
    chmodSync(binary, 0o755);
    writeFileSync(join(stage, "LICENSE"), "MIT fixture\n", "utf8");
    writeFileSync(join(stage, "README.md"), "# Git Slop fixture\n", "utf8");
    mkdirSync(join(stage, "man"), { recursive: true });
    writeFileSync(join(stage, "man", "git-slop.1"), ".TH GIT-SLOP 1\n", "utf8");
    mkdirSync(join(stage, "completions"), { recursive: true });
    for (const shell of ["bash", "zsh", "fish", "powershell", "nushell"]) {
      writeFileSync(
        join(stage, "completions", `git-slop.${shell}`),
        `# ${shell} completion fixture\n`,
        "utf8",
      );
    }
    mkdirSync(join(stage, "schemas"), { recursive: true });
    writeFileSync(join(stage, "schemas", "report-5.json"), "{}\n", "utf8");
    createArchive(archive, ["-c", "-z"], ["-C", join(root, "stage"), stageName]);
    const archiveBytes = readFileSync(archive);
    const digest = createHash("sha256").update(archiveBytes).digest("hex");
    let crateFixtureIndex = 0;
    function buildCrateBytes(crateRevision, dirty = undefined, includeOutsideRoot = false) {
      crateFixtureIndex += 1;
      const fixtureRoot = join(root, `crate-fixture-${crateFixtureIndex}`);
      const packageName = `git-slop-${version}`;
      const packageRoot = join(fixtureRoot, packageName);
      mkdirSync(join(packageRoot, "src"), { recursive: true });
      writeFileSync(
        join(packageRoot, "Cargo.toml"),
        `[package]\nname = "git-slop"\nversion = "${version}"\n`,
        "utf8",
      );
      writeFileSync(join(packageRoot, "src", "main.rs"), "fn main() {}\n", "utf8");
      const git = { sha1: crateRevision };
      if (dirty !== undefined) {
        git.dirty = dirty;
      }
      writeFileSync(
        join(packageRoot, ".cargo_vcs_info.json"),
        `${JSON.stringify({ git, path_in_vcs: "" })}\n`,
        "utf8",
      );
      const members = [packageName];
      if (includeOutsideRoot) {
        mkdirSync(join(fixtureRoot, "outside-package"));
        writeFileSync(join(fixtureRoot, "outside-package", "README.md"), "outside\n", "utf8");
        members.push("outside-package");
      }
      const cratePath = join(root, `fixture-${crateFixtureIndex}.crate`);
      createArchive(cratePath, ["-c", "-z"], ["-C", fixtureRoot, ...members]);
      return readFileSync(cratePath);
    }
    const canonicalCrateBytes = buildCrateBytes(revision);
    const canonicalCrateSha256 = createHash("sha256")
      .update(canonicalCrateBytes)
      .digest("hex");
    let servedArchiveBytes = archiveBytes;
    let servedArchiveDigest = digest;
    let servedCrateBytes = canonicalCrateBytes;
    let crateSha256 = canonicalCrateSha256;
    let crateAuthorizationObserved = null;
    let crateContentLengthOverride = null;
    const draftReleaseId = 365216485;
    const draftReleaseSlug = "untagged-b4f6c1a2d3e4";
    const publishedDownloadBase =
      `https://github.com/example/git-slop/releases/download/${tag}`;
    const draftDownloadBase =
      `https://github.com/example/git-slop/releases/download/${draftReleaseSlug}`;
    let servedReleaseId = draftReleaseId;
    let releaseTagRequests = 0;
    let releaseIdRequests = 0;
    let archiveDownloadRequests = 0;
    let crateDownloadRequests = 0;
    let releaseDraft = false;
    let releaseTagName = tag;
    let releaseAssetDownloadBaseOverride = null;
    let servedTagRevision = revision;
    let tagReferenceType = "commit";
    let tagReferenceName = `refs/tags/${tag}`;
    const annotatedTagSha = "d".repeat(40);
    let checksumBytes;
    let manifestBytes;
    const formulaBytes = Buffer.from(
      `class GitSlop < Formula\n  version "${version}"\nend\n`,
      "utf8",
    );
    const cdxBytes = Buffer.from('{"bomFormat":"CycloneDX"}\n', "utf8");
    const spdxBytes = Buffer.from('{"spdxVersion":"SPDX-2.3"}\n', "utf8");
    let servedFormulaBytes = formulaBytes;
    let manifestMutator = null;
    let assetMutator = null;

    const targetMetadata = {
      "x86_64-unknown-linux-gnu": { os: "linux", arch: "x86_64", archive: "tar.gz" },
      "aarch64-unknown-linux-gnu": { os: "linux", arch: "aarch64", archive: "tar.gz" },
      "aarch64-apple-darwin": { os: "macos", arch: "aarch64", archive: "tar.gz" },
      "x86_64-apple-darwin": { os: "macos", arch: "x86_64", archive: "tar.gz" },
      "x86_64-unknown-linux-musl": { os: "linux", arch: "x86_64", archive: "tar.gz" },
      "x86_64-pc-windows-msvc": { os: "windows", arch: "x86_64", archive: "zip" },
      "aarch64-pc-windows-msvc": { os: "windows", arch: "aarch64", archive: "zip" },
    };

    function buildArtifacts() {
      return Object.entries(targetMetadata).map(([candidateTarget, metadata], index) => {
        const name = `git-slop-${tag}-${candidateTarget}.${metadata.archive}`;
        return {
          name,
          path: name,
          target: candidateTarget,
          os: metadata.os,
          arch: metadata.arch,
          archive: metadata.archive,
          sha256: candidateTarget === target ? servedArchiveDigest : String(index + 1).repeat(64),
          size_bytes: candidateTarget === target ? servedArchiveBytes.length : index + 1,
          url: `https://github.com/example/git-slop/releases/download/${tag}/${name}`,
        };
      });
    }

    function buildManifestBytes() {
      const artifacts = buildArtifacts();
      const manifest = structuredClone(releaseManifestFixture);
      Object.assign(manifest, {
        schema_version: 3,
        project: "git-slop",
        version,
        tag,
        revision,
        repository: "example/git-slop",
        artifacts,
        supplemental_assets: [
          ["git-slop.rb", "homebrew_formula", "text/x-ruby", formulaBytes],
          ["git-slop.cdx.json", "cyclonedx_sbom", "application/vnd.cyclonedx+json", cdxBytes],
          ["git-slop.spdx.json", "spdx_sbom", "application/spdx+json", spdxBytes],
        ].map(([name, role, media_type, bytes]) => ({
          name,
          path: name,
          role,
          media_type,
          required: true,
          contract_version: 1,
          sha256: createHash("sha256").update(bytes).digest("hex"),
          size_bytes: bytes.length,
          url: `https://github.com/example/git-slop/releases/download/${tag}/${name}`,
        })),
      });
      Object.assign(manifest.crate_source, {
        schema_version: 1,
        registry: "crates.io",
        package: "git-slop",
        version,
        url: `https://static.crates.io/crates/git-slop/git-slop-${version}.crate`,
        sha256: crateSha256,
        revision,
        vcs_dirty: false,
      });
      Object.assign(manifest.checksums, {
        algorithm: "sha256",
        name: "SHA256SUMS",
        url: `https://github.com/example/git-slop/releases/download/${tag}/SHA256SUMS`,
      });
      manifest.install.github_release = [
        `gh release download ${tag} --repo example/git-slop --pattern 'git-slop-${tag}-<target>.*' --pattern SHA256SUMS`,
        "sha256sum --check SHA256SUMS --ignore-missing",
      ];
      manifest.install.attestation = [
        `gh attestation verify 'git-slop-${tag}-<target>.*' --repo example/git-slop --signer-repo example/git-slop`,
      ];
      manifest.install.cargo = [`cargo install git-slop --version ${version} --locked`];
      if (manifestMutator) {
        manifestMutator(manifest);
      }
      return Buffer.from(
        `${JSON.stringify(manifest)}\n`,
        "utf8",
      );
    }

    function buildReleaseAssets() {
      const browserDownloadBase =
        releaseAssetDownloadBaseOverride ??
        (releaseDraft ? draftDownloadBase : publishedDownloadBase);
      const artifacts = buildArtifacts().map((artifact) => ({
        name: artifact.name,
        size: artifact.size_bytes,
        digest: `sha256:${artifact.sha256}`,
        url:
          artifact.target === target
            ? `${apiRoot}/assets/archive`
            : `${apiRoot}/assets/${encodeURIComponent(artifact.name)}`,
        browser_download_url: `${browserDownloadBase}/${artifact.name}`,
      }));
      const metadataAssets = [
        {
          name: "SHA256SUMS",
          bytes: checksumBytes,
          url: `${apiRoot}/assets/checksums`,
        },
        {
          name: "git-slop.rb",
          bytes: formulaBytes,
          url: `${apiRoot}/assets/formula`,
        },
        {
          name: "git-slop.cdx.json",
          bytes: cdxBytes,
          url: `${apiRoot}/assets/cdx`,
        },
        {
          name: "git-slop.spdx.json",
          bytes: spdxBytes,
          url: `${apiRoot}/assets/spdx`,
        },
        {
          name: "release-manifest.json",
          bytes: manifestBytes,
          url: `${apiRoot}/assets/manifest`,
        },
      ].map(({ name, bytes, url }) => ({
        name,
        size: bytes.length,
        digest: `sha256:${createHash("sha256").update(bytes).digest("hex")}`,
        url,
        browser_download_url: `${browserDownloadBase}/${name}`,
      }));
      const assets = [...artifacts, ...metadataAssets];
      if (assetMutator) {
        assetMutator(assets);
      }
      return assets;
    }

    let apiRoot;
    const server = createServer((request, response) => {
      if (request.url === `/repos/example/git-slop/releases/tags/${tag}`) {
        releaseTagRequests += 1;
        if (releaseDraft) {
          response.statusCode = 404;
          response.end("not found");
          return;
        }
        response.setHeader("content-type", "application/json");
        response.end(
          JSON.stringify({
            id: servedReleaseId,
            tag_name: releaseTagName,
            draft: releaseDraft,
            prerelease: false,
            published_at: releaseDraft ? null : "2026-08-04T18:00:00Z",
            html_url: `https://github.com/example/git-slop/releases/tag/${
              releaseDraft ? draftReleaseSlug : tag
            }`,
            assets: buildReleaseAssets(),
          }),
        );
      } else if (request.url === `/repos/example/git-slop/releases/${draftReleaseId}`) {
        releaseIdRequests += 1;
        response.setHeader("content-type", "application/json");
        response.end(
          JSON.stringify({
            id: servedReleaseId,
            tag_name: releaseTagName,
            draft: releaseDraft,
            prerelease: false,
            published_at: releaseDraft ? null : "2026-08-04T18:00:00Z",
            html_url: `https://github.com/example/git-slop/releases/tag/${
              releaseDraft ? draftReleaseSlug : tag
            }`,
            assets: buildReleaseAssets(),
          }),
        );
      } else if (request.url === `/repos/example/git-slop/git/ref/tags/${tag}`) {
        response.setHeader("content-type", "application/json");
        response.end(
          JSON.stringify({
            ref: tagReferenceName,
            object: {
              type: tagReferenceType,
              sha: tagReferenceType === "commit" ? servedTagRevision : annotatedTagSha,
            },
          }),
        );
      } else if (request.url === `/repos/example/git-slop/git/tags/${annotatedTagSha}`) {
        response.setHeader("content-type", "application/json");
        response.end(
          JSON.stringify({
            sha: annotatedTagSha,
            tag,
            object: { type: "commit", sha: servedTagRevision },
          }),
        );
      } else if (request.url === "/assets/archive") {
        archiveDownloadRequests += 1;
        response.end(servedArchiveBytes);
      } else if (request.url === "/assets/checksums") {
        response.end(checksumBytes);
      } else if (request.url === "/assets/manifest") {
        response.end(manifestBytes);
      } else if (request.url === "/assets/formula") {
        response.end(servedFormulaBytes);
      } else if (request.url === `/crates/git-slop/git-slop-${version}.crate`) {
        crateDownloadRequests += 1;
        crateAuthorizationObserved = request.headers.authorization ?? null;
        if (crateContentLengthOverride !== null) {
          response.setHeader("content-length", crateContentLengthOverride);
        }
        response.end(servedCrateBytes);
      } else {
        response.statusCode = 404;
        response.end("not found");
      }
    });
    await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
    const address = server.address();
    apiRoot = `http://127.0.0.1:${address.port}`;
    const installerHarness = join(root, "installer-harness.mjs");
    writeFileSync(
      installerHarness,
      `const originalFetch = globalThis.fetch;
globalThis.fetch = (input, options) => {
  const url = String(input);
  if (url.startsWith("https://static.crates.io/")) {
    return originalFetch(process.env.GIT_SLOP_TEST_CRATES_ORIGIN + new URL(url).pathname, options);
  }
  return originalFetch(input, options);
};
const { main } = await import(${JSON.stringify(pathToFileURL(installer).href)});
try {
  await main();
} catch (error) {
  console.error("git-slop Action installation failed: " + (error instanceof Error ? error.message : String(error)));
  process.exitCode = 1;
}
`,
      "utf8",
    );
    const runInstaller = (environment) =>
      runNode(installerHarness, {
        ...environment,
        GIT_SLOP_TEST_CRATES_ORIGIN: apiRoot,
      });
    function refreshMetadata() {
      manifestBytes = buildManifestBytes();
      const manifestDigest = createHash("sha256").update(manifestBytes).digest("hex");
      const manifest = JSON.parse(manifestBytes.toString("utf8"));
      const artifactChecksums = manifest.artifacts
        .map((artifact) => `${artifact.sha256}  ${artifact.name}`)
        .join("\n");
      const cdxDigest = createHash("sha256").update(cdxBytes).digest("hex");
      const spdxDigest = createHash("sha256").update(spdxBytes).digest("hex");
      checksumBytes = Buffer.from(
        `${artifactChecksums}\n${createHash("sha256").update(formulaBytes).digest("hex")}  git-slop.rb\n${cdxDigest}  git-slop.cdx.json\n${spdxDigest}  git-slop.spdx.json\n${manifestDigest}  release-manifest.json\n`,
        "utf8",
      );
    }
    refreshMetadata();

    const scenarioContext = {
      apiRoot,
      archiveBytes,
      assetName,
      buildCrateBytes,
      canonicalCrateBytes,
      canonicalCrateSha256,
      createArchive,
      digest,
      draftDownloadBase,
      draftReleaseId,
      draftReleaseSlug,
      formulaBytes,
      githubPath,
      output,
      outputs,
      publishedDownloadBase,
      refreshMetadata,
      revision,
      root,
      runInstaller,
      stage,
      stageName,
      tag,
      target,
      validateArchiveFormat,
      version,
      get archiveDownloadRequests() {
        return archiveDownloadRequests;
      },
      get assetMutator() {
        return assetMutator;
      },
      set assetMutator(value) {
        assetMutator = value;
      },
      get checksumBytes() {
        return checksumBytes;
      },
      set checksumBytes(value) {
        checksumBytes = value;
      },
      get crateAuthorizationObserved() {
        return crateAuthorizationObserved;
      },
      get crateContentLengthOverride() {
        return crateContentLengthOverride;
      },
      set crateContentLengthOverride(value) {
        crateContentLengthOverride = value;
      },
      get crateDownloadRequests() {
        return crateDownloadRequests;
      },
      get crateSha256() {
        return crateSha256;
      },
      set crateSha256(value) {
        crateSha256 = value;
      },
      get manifestMutator() {
        return manifestMutator;
      },
      set manifestMutator(value) {
        manifestMutator = value;
      },
      get releaseAssetDownloadBaseOverride() {
        return releaseAssetDownloadBaseOverride;
      },
      set releaseAssetDownloadBaseOverride(value) {
        releaseAssetDownloadBaseOverride = value;
      },
      get releaseDraft() {
        return releaseDraft;
      },
      set releaseDraft(value) {
        releaseDraft = value;
      },
      get releaseIdRequests() {
        return releaseIdRequests;
      },
      get releaseTagName() {
        return releaseTagName;
      },
      set releaseTagName(value) {
        releaseTagName = value;
      },
      get releaseTagRequests() {
        return releaseTagRequests;
      },
      get servedArchiveBytes() {
        return servedArchiveBytes;
      },
      set servedArchiveBytes(value) {
        servedArchiveBytes = value;
      },
      get servedArchiveDigest() {
        return servedArchiveDigest;
      },
      set servedArchiveDigest(value) {
        servedArchiveDigest = value;
      },
      get servedCrateBytes() {
        return servedCrateBytes;
      },
      set servedCrateBytes(value) {
        servedCrateBytes = value;
      },
      get servedFormulaBytes() {
        return servedFormulaBytes;
      },
      set servedFormulaBytes(value) {
        servedFormulaBytes = value;
      },
      get servedReleaseId() {
        return servedReleaseId;
      },
      set servedReleaseId(value) {
        servedReleaseId = value;
      },
      get servedTagRevision() {
        return servedTagRevision;
      },
      set servedTagRevision(value) {
        servedTagRevision = value;
      },
      get tagReferenceName() {
        return tagReferenceName;
      },
      set tagReferenceName(value) {
        tagReferenceName = value;
      },
      get tagReferenceType() {
        return tagReferenceType;
      },
      set tagReferenceType(value) {
        tagReferenceType = value;
      },
    };

    try {
      await exerciseInstallAndCache(scenarioContext);
      await exerciseDraftRelease(scenarioContext);
      await exerciseTagIdentity(scenarioContext);
      await exerciseCrateSource(scenarioContext);
      await exerciseReleaseAssets(scenarioContext);
      await exerciseArchiveLayout(scenarioContext);
      await exerciseReleaseManifest(scenarioContext);
    } finally {
      await new Promise((resolve, reject) =>
        server.close((error) => (error ? reject(error) : resolve())),
      );
    }
  },
);

test("canonical crates.io packages support absolute archive paths", () => {
  const root = mkdtempSync(join(tmpdir(), "git-slop-canonical-crate-test-"));
  const version = "0.9.0";
  const revision = "a".repeat(40);
  const rootName = `git-slop-${version}`;
  const packageRoot = join(root, rootName);
  mkdirSync(join(packageRoot, "src"), { recursive: true });
  writeFileSync(
    join(packageRoot, "Cargo.toml"),
    `[package]\nname = "git-slop"\nversion = "${version}"\n`,
    "utf8",
  );
  writeFileSync(join(packageRoot, "src", "main.rs"), "fn main() {}\n", "utf8");
  writeFileSync(
    join(packageRoot, ".cargo_vcs_info.json"),
    `${JSON.stringify({ git: { sha1: revision }, path_in_vcs: "" })}\n`,
    "utf8",
  );

  const archive = join(root, `${rootName}.crate`);
  assert.equal(isAbsolute(archive), true);
  if (process.platform === "win32") {
    assert.match(archive, /^[A-Za-z]:[\\/]/u);
  }
  createArchive(archive, ["-c", "-z"], ["-C", root, rootName]);
  const bytes = readFileSync(archive);
  const digest = createHash("sha256").update(bytes).digest("hex");

  assert.equal(verifyCanonicalCrate(archive, bytes, version, digest, revision), digest);
});

test("Windows ZIP archives use the exact safe layout when the host tar supports ZIP", (context) => {
  const root = mkdtempSync(join(tmpdir(), "git-slop-windows-zip-test-"));
  const rootName = "git-slop-v0.9.0-x86_64-pc-windows-msvc";
  const stageParent = join(root, "stage");
  const stage = join(stageParent, rootName);
  mkdirSync(stage, { recursive: true });
  const payloads = {
    "git-slop.exe": "fixture executable\n",
    LICENSE: "MIT fixture\n",
    "README.md": "# Git Slop fixture\n",
    "man/git-slop.1": ".TH GIT-SLOP 1\n",
    "completions/git-slop.bash": "# bash completion fixture\n",
    "completions/git-slop.zsh": "# zsh completion fixture\n",
    "completions/git-slop.fish": "# fish completion fixture\n",
    "completions/git-slop.powershell": "# powershell completion fixture\n",
    "completions/git-slop.nushell": "# nushell completion fixture\n",
    "schemas/report-5.json": "{}\n",
  };
  for (const [name, contents] of Object.entries(payloads)) {
    mkdirSync(dirname(join(stage, name)), { recursive: true });
    writeFileSync(join(stage, name), contents, "utf8");
  }
  const archive = join(root, `${rootName}.zip`);
  if (process.platform === "win32") {
    assert.match(archive, /^[A-Za-z]:[\\/]/u);
  }
  const archiveIsZip = () => {
    if (!existsSync(archive)) {
      return false;
    }
    try {
      validateArchiveFormat(readFileSync(archive), "zip");
      return true;
    } catch {
      return false;
    }
  };
  let zipped = runArchiveTar(archive, ["-a", "-c"], ["-C", stageParent, rootName], {
    encoding: "utf8",
  });
  if ((zipped.status !== 0 || !archiveIsZip()) && process.platform !== "win32") {
    rmSync(archive, { force: true });
    zipped = spawnSync("zip", ["-q", "-D", "-r", archive, rootName], {
      cwd: stageParent,
      encoding: "utf8",
    });
  }
  if (zipped.status !== 0 || !archiveIsZip()) {
    if (process.platform === "win32") {
      assert.fail(`Windows built-in tar.exe could not create the ZIP archive: ${zipped.stderr}`);
    }
    context.skip(`ZIP creation is unavailable: ${zipped.stderr}`);
    return;
  }
  validateArchiveFormat(readFileSync(archive), "zip");
  const inventory = runArchiveTar(archive, ["-t"], [], { encoding: "utf8" });
  if (inventory.status !== 0) {
    if (process.platform === "win32") {
      assert.fail(`Windows built-in tar.exe could not inspect the ZIP archive: ${inventory.stderr}`);
    }
    context.skip("the host tar does not support ZIP archives");
    return;
  }
  const installRoot = join(root, "install");
  mkdirSync(installRoot);
  const binary = materializeArchive(archive, installRoot, rootName, "git-slop.exe");
  assert.equal(readFileSync(binary, "utf8"), payloads["git-slop.exe"]);
});
