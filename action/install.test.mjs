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
      env: { ...process.env, ...environment },
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
    writeFileSync(join(stage, "git-slop.1"), ".TH GIT-SLOP 1\n", "utf8");
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
        response.end(servedArchiveBytes);
      } else if (request.url === "/assets/checksums") {
        response.end(checksumBytes);
      } else if (request.url === "/assets/manifest") {
        response.end(manifestBytes);
      } else if (request.url === "/assets/formula") {
        response.end(servedFormulaBytes);
      } else if (request.url === `/crates/git-slop/git-slop-${version}.crate`) {
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
      checksumBytes = Buffer.from(
        `${artifactChecksums}\n${createHash("sha256").update(formulaBytes).digest("hex")}  git-slop.rb\n${manifestDigest}  release-manifest.json\n`,
        "utf8",
      );
    }
    refreshMetadata();

    try {
      const installed = await runInstaller({
        GITHUB_API_URL: apiRoot,
        GITHUB_OUTPUT: output,
        GITHUB_PATH: githubPath,
        GIT_SLOP_ACTION_VERSION: version,
        GIT_SLOP_GITHUB_TOKEN: "github-test-token",
        GIT_SLOP_RELEASE_REPOSITORY: "example/git-slop",
        RUNNER_TEMP: root,
      });
      assert.equal(installed.status, 0, installed.stderr);
      const actual = outputs(output);
      assert.equal(actual.version, version);
      assert.equal(actual.target, target);
      assert.equal(actual.asset, assetName);
      assert.equal(actual["asset-url"], `${publishedDownloadBase}/${assetName}`);
      assert.equal(actual.sha256, digest);
      assert.equal(actual["source-revision"], revision);
      assert.equal(actual["crate-sha256"], crateSha256);
      assert.match(actual["release-manifest-sha256"], /^[a-f0-9]{64}$/u);
      assert.ok(existsSync(actual["binary-path"]));
      assert.equal(readFileSync(githubPath, "utf8").trim(), dirname(actual["binary-path"]));
      assert.equal(crateAuthorizationObserved, null, "GitHub token leaked to crates.io fetch");
      assert.equal(releaseTagRequests, 1);
      assert.equal(releaseIdRequests, 0);

      releaseDraft = true;
      const rejectedDraft = await runInstaller({
        GITHUB_API_URL: apiRoot,
        GIT_SLOP_ACTION_VERSION: version,
        GIT_SLOP_RELEASE_REPOSITORY: "example/git-slop",
        RUNNER_TEMP: root,
      });
      assert.equal(rejectedDraft.status, 1);
      assert.match(rejectedDraft.stderr, /returned HTTP 404/u);

      const rejectedDraftWithoutId = await runInstaller({
        GITHUB_API_URL: apiRoot,
        GITHUB_REPOSITORY: "example/git-slop",
        GIT_SLOP_ACTION_VERSION: version,
        GIT_SLOP_GITHUB_TOKEN: "github-test-token",
        GIT_SLOP_RELEASE_REPOSITORY: "example/git-slop",
        GIT_SLOP_ALLOW_DRAFT_RELEASE: "true",
        RUNNER_TEMP: root,
      });
      assert.equal(rejectedDraftWithoutId.status, 1);
      assert.match(rejectedDraftWithoutId.stderr, /requires an exact release ID/u);

      const draftOutput = join(root, "github-draft-output.txt");
      const acceptedDraft = await runInstaller({
        GITHUB_API_URL: apiRoot,
        GITHUB_OUTPUT: draftOutput,
        GITHUB_REPOSITORY: "example/git-slop",
        GIT_SLOP_ACTION_VERSION: version,
        GIT_SLOP_GITHUB_TOKEN: "github-test-token",
        GIT_SLOP_RELEASE_REPOSITORY: "example/git-slop",
        GIT_SLOP_ALLOW_DRAFT_RELEASE: "true",
        GIT_SLOP_RELEASE_ID: String(draftReleaseId),
        RUNNER_TEMP: root,
      });
      assert.equal(acceptedDraft.status, 0, acceptedDraft.stderr);
      assert.equal(outputs(draftOutput)["asset-url"], `${draftDownloadBase}/${assetName}`);
      assert.equal(releaseIdRequests, 1);

      const invalidDraftAssetDownloadBases = [
        [
          "slug",
          "https://github.com/example/git-slop/releases/download/untagged-e5d4c3b2a1f0",
        ],
        [
          "repository",
          `https://github.com/example/not-git-slop/releases/download/${draftReleaseSlug}`,
        ],
        [
          "path",
          `https://github.com/example/git-slop/releases/assets/${draftReleaseSlug}`,
        ],
      ];
      for (const [label, downloadBase] of invalidDraftAssetDownloadBases) {
        releaseAssetDownloadBaseOverride = downloadBase;
        const invalidDraftAssetUrl = await runInstaller({
          GITHUB_API_URL: apiRoot,
          GITHUB_REPOSITORY: "example/git-slop",
          GIT_SLOP_ACTION_VERSION: version,
          GIT_SLOP_GITHUB_TOKEN: "github-test-token",
          GIT_SLOP_RELEASE_REPOSITORY: "example/git-slop",
          GIT_SLOP_ALLOW_DRAFT_RELEASE: "true",
          GIT_SLOP_RELEASE_ID: String(draftReleaseId),
          RUNNER_TEMP: root,
        });
        assert.equal(invalidDraftAssetUrl.status, 1, `${label}: ${invalidDraftAssetUrl.stderr}`);
        assert.match(invalidDraftAssetUrl.stderr, /invalid metadata for asset/u);
      }
      releaseAssetDownloadBaseOverride = null;

      const rejectedMalformedDraftId = await runInstaller({
        GITHUB_API_URL: apiRoot,
        GITHUB_REPOSITORY: "example/git-slop",
        GIT_SLOP_ACTION_VERSION: version,
        GIT_SLOP_GITHUB_TOKEN: "github-test-token",
        GIT_SLOP_RELEASE_REPOSITORY: "example/git-slop",
        GIT_SLOP_ALLOW_DRAFT_RELEASE: "true",
        GIT_SLOP_RELEASE_ID: "not-a-release-id",
        RUNNER_TEMP: root,
      });
      assert.equal(rejectedMalformedDraftId.status, 1);
      assert.match(rejectedMalformedDraftId.stderr, /positive decimal integer/u);

      const rejectedUnsafeDraftId = await runInstaller({
        GITHUB_API_URL: apiRoot,
        GITHUB_REPOSITORY: "example/git-slop",
        GIT_SLOP_ACTION_VERSION: version,
        GIT_SLOP_GITHUB_TOKEN: "github-test-token",
        GIT_SLOP_RELEASE_REPOSITORY: "example/git-slop",
        GIT_SLOP_ALLOW_DRAFT_RELEASE: "true",
        GIT_SLOP_RELEASE_ID: "9007199254740992",
        RUNNER_TEMP: root,
      });
      assert.equal(rejectedUnsafeDraftId.status, 1);
      assert.match(rejectedUnsafeDraftId.stderr, /safe integer range/u);

      const rejectedUnauthenticatedDraftId = await runInstaller({
        GITHUB_API_URL: apiRoot,
        GITHUB_REPOSITORY: "example/git-slop",
        GIT_SLOP_ACTION_VERSION: version,
        GIT_SLOP_RELEASE_REPOSITORY: "example/git-slop",
        GIT_SLOP_ALLOW_DRAFT_RELEASE: "true",
        GIT_SLOP_RELEASE_ID: String(draftReleaseId),
        RUNNER_TEMP: root,
      });
      assert.equal(rejectedUnauthenticatedDraftId.status, 1);
      assert.match(
        rejectedUnauthenticatedDraftId.stderr,
        /restricted to authenticated same-repository/u,
      );

      servedReleaseId = draftReleaseId + 1;
      const rejectedMismatchedDraftId = await runInstaller({
        GITHUB_API_URL: apiRoot,
        GITHUB_REPOSITORY: "example/git-slop",
        GIT_SLOP_ACTION_VERSION: version,
        GIT_SLOP_GITHUB_TOKEN: "github-test-token",
        GIT_SLOP_RELEASE_REPOSITORY: "example/git-slop",
        GIT_SLOP_ALLOW_DRAFT_RELEASE: "true",
        GIT_SLOP_RELEASE_ID: String(draftReleaseId),
        RUNNER_TEMP: root,
      });
      assert.equal(rejectedMismatchedDraftId.status, 1);
      assert.match(rejectedMismatchedDraftId.stderr, /inconsistent GitHub release metadata/u);
      servedReleaseId = draftReleaseId;

      const rejectedConsumerDraft = await runInstaller({
        GITHUB_API_URL: apiRoot,
        GITHUB_REPOSITORY: "example/consumer",
        GIT_SLOP_ACTION_VERSION: version,
        GIT_SLOP_GITHUB_TOKEN: "github-test-token",
        GIT_SLOP_RELEASE_REPOSITORY: "example/git-slop",
        GIT_SLOP_ALLOW_DRAFT_RELEASE: "true",
        GIT_SLOP_RELEASE_ID: String(draftReleaseId),
        RUNNER_TEMP: root,
      });
      assert.equal(rejectedConsumerDraft.status, 1);
      assert.match(rejectedConsumerDraft.stderr, /restricted to authenticated same-repository/u);

      releaseDraft = false;
      releaseAssetDownloadBaseOverride = draftDownloadBase;
      const rejectedPublishedDraftAssetUrl = await runInstaller({
        GITHUB_API_URL: apiRoot,
        GIT_SLOP_ACTION_VERSION: version,
        GIT_SLOP_RELEASE_REPOSITORY: "example/git-slop",
        RUNNER_TEMP: root,
      });
      assert.equal(rejectedPublishedDraftAssetUrl.status, 1);
      assert.match(rejectedPublishedDraftAssetUrl.stderr, /invalid metadata for asset/u);
      releaseAssetDownloadBaseOverride = null;

      releaseTagName = "v0.9.1";
      const rejectedTagMetadata = await runInstaller({
        GITHUB_API_URL: apiRoot,
        GIT_SLOP_ACTION_VERSION: version,
        GIT_SLOP_RELEASE_REPOSITORY: "example/git-slop",
        RUNNER_TEMP: root,
      });
      assert.equal(rejectedTagMetadata.status, 1);
      assert.match(rejectedTagMetadata.stderr, /inconsistent GitHub release metadata/u);
      releaseTagName = tag;

      servedTagRevision = "c".repeat(40);
      const rejectedTagRevision = await runInstaller({
        GITHUB_API_URL: apiRoot,
        GIT_SLOP_ACTION_VERSION: version,
        GIT_SLOP_RELEASE_REPOSITORY: "example/git-slop",
        RUNNER_TEMP: root,
      });
      assert.equal(rejectedTagRevision.status, 1);
      assert.match(rejectedTagRevision.stderr, /release tag .* resolves to/u);
      servedTagRevision = revision;

      tagReferenceType = "tag";
      const acceptedAnnotatedTag = await runInstaller({
        GITHUB_API_URL: apiRoot,
        GIT_SLOP_ACTION_VERSION: version,
        GIT_SLOP_RELEASE_REPOSITORY: "example/git-slop",
        RUNNER_TEMP: root,
      });
      assert.equal(acceptedAnnotatedTag.status, 0, acceptedAnnotatedTag.stderr);
      tagReferenceType = "commit";

      tagReferenceName = `refs/heads/${tag}`;
      const rejectedBranchAlias = await runInstaller({
        GITHUB_API_URL: apiRoot,
        GIT_SLOP_ACTION_VERSION: version,
        GIT_SLOP_RELEASE_REPOSITORY: "example/git-slop",
        RUNNER_TEMP: root,
      });
      assert.equal(rejectedBranchAlias.status, 1);
      assert.match(rejectedBranchAlias.stderr, /invalid exact-reference metadata/u);
      tagReferenceName = `refs/tags/${tag}`;

      servedCrateBytes = Buffer.concat([canonicalCrateBytes, Buffer.from("unexpected")]);
      const rejectedCrateDigest = await runInstaller({
        GITHUB_API_URL: apiRoot,
        GIT_SLOP_ACTION_VERSION: version,
        GIT_SLOP_RELEASE_REPOSITORY: "example/git-slop",
        RUNNER_TEMP: root,
      });
      assert.equal(rejectedCrateDigest.status, 1);
      assert.match(rejectedCrateDigest.stderr, /package SHA-256 mismatch/u);

      servedCrateBytes = canonicalCrateBytes;
      crateContentLengthOverride = 16 * 1024 * 1024 + 1;
      const rejectedOversizedCrate = await runInstaller({
        GITHUB_API_URL: apiRoot,
        GIT_SLOP_ACTION_VERSION: version,
        GIT_SLOP_RELEASE_REPOSITORY: "example/git-slop",
        RUNNER_TEMP: root,
      });
      assert.equal(rejectedOversizedCrate.status, 1);
      assert.match(rejectedOversizedCrate.stderr, /exceeds its download size limit/u);
      crateContentLengthOverride = null;

      servedCrateBytes = buildCrateBytes("c".repeat(40));
      crateSha256 = createHash("sha256").update(servedCrateBytes).digest("hex");
      refreshMetadata();
      const rejectedCrateRevision = await runInstaller({
        GITHUB_API_URL: apiRoot,
        GIT_SLOP_ACTION_VERSION: version,
        GIT_SLOP_RELEASE_REPOSITORY: "example/git-slop",
        RUNNER_TEMP: root,
      });
      assert.equal(rejectedCrateRevision.status, 1);
      assert.match(rejectedCrateRevision.stderr, /VCS metadata does not match/u);

      servedCrateBytes = buildCrateBytes(revision, true);
      crateSha256 = createHash("sha256").update(servedCrateBytes).digest("hex");
      refreshMetadata();
      const rejectedDirtyCrate = await runInstaller({
        GITHUB_API_URL: apiRoot,
        GIT_SLOP_ACTION_VERSION: version,
        GIT_SLOP_RELEASE_REPOSITORY: "example/git-slop",
        RUNNER_TEMP: root,
      });
      assert.equal(rejectedDirtyCrate.status, 1);
      assert.match(rejectedDirtyCrate.stderr, /VCS metadata does not match/u);

      servedCrateBytes = buildCrateBytes(revision, false, true);
      crateSha256 = createHash("sha256").update(servedCrateBytes).digest("hex");
      refreshMetadata();
      const rejectedCrateRootEscape = await runInstaller({
        GITHUB_API_URL: apiRoot,
        GIT_SLOP_ACTION_VERSION: version,
        GIT_SLOP_RELEASE_REPOSITORY: "example/git-slop",
        RUNNER_TEMP: root,
      });
      assert.equal(rejectedCrateRootEscape.status, 1);
      assert.match(rejectedCrateRootEscape.stderr, /escapes the expected root/u);

      servedCrateBytes = canonicalCrateBytes;
      crateSha256 = canonicalCrateSha256;
      refreshMetadata();

      const rejectedNoncanonicalVersion = await runInstaller({
        GITHUB_API_URL: apiRoot,
        GIT_SLOP_ACTION_VERSION: "01.2.3",
        GIT_SLOP_RELEASE_REPOSITORY: "example/git-slop",
        RUNNER_TEMP: root,
      });
      assert.equal(rejectedNoncanonicalVersion.status, 1);
      assert.match(rejectedNoncanonicalVersion.stderr, /invalid release version/u);

      const zipArchive = join(root, "zip-fixture.zip");
      const zipped = spawnSync("zip", ["-q", "-D", "-r", zipArchive, stageName], {
        cwd: join(root, "stage"),
        encoding: "utf8",
      });
      if (zipped.status === 0) {
        servedArchiveBytes = readFileSync(zipArchive);
        servedArchiveDigest = createHash("sha256").update(servedArchiveBytes).digest("hex");
        refreshMetadata();
        const wrongArchiveFormat = await runInstaller({
          GITHUB_API_URL: apiRoot,
          GIT_SLOP_ACTION_VERSION: version,
          GIT_SLOP_RELEASE_REPOSITORY: "example/git-slop",
          RUNNER_TEMP: root,
        });
        assert.equal(wrongArchiveFormat.status, 1);
        assert.match(wrongArchiveFormat.stderr, /required tar\.gz format/u);

        validateArchiveFormat(servedArchiveBytes, "zip");
        assert.throws(() => validateArchiveFormat(servedArchiveBytes, "tar.gz"), /tar\.gz/u);
        assert.throws(() => validateArchiveFormat(archiveBytes, "zip"), /ZIP/u);
      }

      servedArchiveBytes = archiveBytes;
      servedArchiveDigest = digest;
      refreshMetadata();
      checksumBytes = Buffer.concat([
        checksumBytes,
        Buffer.from(`${servedArchiveDigest}  ${assetName}\n`, "utf8"),
      ]);
      const duplicateChecksums = await runInstaller({
        GITHUB_API_URL: apiRoot,
        GIT_SLOP_ACTION_VERSION: version,
        GIT_SLOP_RELEASE_REPOSITORY: "example/git-slop",
        RUNNER_TEMP: root,
      });
      assert.equal(duplicateChecksums.status, 1);
      assert.match(duplicateChecksums.stderr, /duplicate entry/u);

      refreshMetadata();
      const invalidAssetSets = [
        ["missing", (assets) => assets.pop(), /exactly ten/u],
        [
          "extra",
          (assets) => assets.push({ ...assets[0], name: "unexpected.bin" }),
          /exactly ten/u,
        ],
        [
          "duplicate",
          (assets) => {
            assets[1] = { ...assets[0] };
          },
          /duplicate asset/u,
        ],
        [
          "unselected digest",
          (assets) => {
            const candidate = assets.find(
              (entry) => entry.name.endsWith(".tar.gz") && entry.name !== assetName,
            );
            candidate.digest = `sha256:${"9".repeat(64)}`;
          },
          /does not authenticate/u,
        ],
        [
          "unselected size",
          (assets) => {
            const candidate = assets.find(
              (entry) => entry.name.endsWith(".zip") && entry.name !== assetName,
            );
            candidate.size += 1;
          },
          /does not authenticate/u,
        ],
        [
          "oversized metadata",
          (assets) => {
            assets.find((entry) => entry.name === "git-slop.rb").size = 1024 * 1024 + 1;
          },
          /invalid metadata/u,
        ],
      ];
      for (const [label, mutate, message] of invalidAssetSets) {
        assetMutator = mutate;
        const invalidAssets = await runInstaller({
          GITHUB_API_URL: apiRoot,
          GIT_SLOP_ACTION_VERSION: version,
          GIT_SLOP_RELEASE_REPOSITORY: "example/git-slop",
          RUNNER_TEMP: root,
        });
        assert.equal(invalidAssets.status, 1, `${label}: ${invalidAssets.stderr}`);
        assert.match(invalidAssets.stderr, message);
      }
      assetMutator = null;

      servedFormulaBytes = Buffer.concat([formulaBytes, Buffer.from("unexpected")]);
      const oversizedFormulaResponse = await runInstaller({
        GITHUB_API_URL: apiRoot,
        GIT_SLOP_ACTION_VERSION: version,
        GIT_SLOP_RELEASE_REPOSITORY: "example/git-slop",
        RUNNER_TEMP: root,
      });
      assert.equal(oversizedFormulaResponse.status, 1);
      assert.match(
        oversizedFormulaResponse.stderr,
        /Content-Length mismatch|exceeds its download size limit/u,
      );
      servedFormulaBytes = formulaBytes;

      const formulaDigest = createHash("sha256").update(formulaBytes).digest("hex");
      checksumBytes = Buffer.from(
        checksumBytes.toString("utf8").replace(formulaDigest, "e".repeat(64)),
        "utf8",
      );
      const mismatchedFormulaChecksum = await runInstaller({
        GITHUB_API_URL: apiRoot,
        GIT_SLOP_ACTION_VERSION: version,
        GIT_SLOP_RELEASE_REPOSITORY: "example/git-slop",
        RUNNER_TEMP: root,
      });
      assert.equal(mismatchedFormulaChecksum.status, 1);
      assert.match(mismatchedFormulaChecksum.stderr, /does not authenticate git-slop\.rb/u);
      refreshMetadata();

      writeFileSync(join(stage, "UNEXPECTED"), "unsafe extra member\n", "utf8");
      const unsafeArchive = join(root, `unsafe-${assetName}`);
      createArchive(
        unsafeArchive,
        ["-c", "-z"],
        ["-C", join(root, "stage"), stageName],
      );
      servedArchiveBytes = readFileSync(unsafeArchive);
      servedArchiveDigest = createHash("sha256").update(servedArchiveBytes).digest("hex");
      refreshMetadata();
      const unsafeInventory = await runInstaller({
        GITHUB_API_URL: apiRoot,
        GIT_SLOP_ACTION_VERSION: version,
        GIT_SLOP_RELEASE_REPOSITORY: "example/git-slop",
        RUNNER_TEMP: root,
      });
      assert.equal(unsafeInventory.status, 1);
      assert.match(unsafeInventory.stderr, /exact Git Slop release layout/u);

      servedArchiveBytes = archiveBytes;
      servedArchiveDigest = digest;
      const invalidManifests = [
        ["extra", (manifest) => {
          const extra = { ...manifest.artifacts[0] };
          extra.target = "riscv64gc-unknown-linux-gnu";
          extra.name = `git-slop-${tag}-${extra.target}.tar.gz`;
          extra.path = extra.name;
          extra.url = `https://github.com/example/git-slop/releases/download/${tag}/${extra.name}`;
          manifest.artifacts.push(extra);
        }],
        ["missing", (manifest) => manifest.artifacts.pop()],
        ["duplicate", (manifest) => { manifest.artifacts[1].target = manifest.artifacts[0].target; }],
        ["unknown", (manifest) => { manifest.artifacts[0].target = "riscv64gc-unknown-linux-gnu"; }],
        ["wrong version", (manifest) => { manifest.version = "0.9.1"; }],
        ["wrong tag", (manifest) => { manifest.tag = "v0.9.1"; }],
        ["short revision", (manifest) => { manifest.revision = "abc123"; }],
        ["missing install", (manifest) => { delete manifest.install; }],
        ["unknown top-level field", (manifest) => { manifest.unexpected = true; }],
        ["unknown crate field", (manifest) => { manifest.crate_source.unexpected = true; }],
        ["unknown artifact field", (manifest) => { manifest.artifacts[0].unexpected = true; }],
        ["unknown checksum field", (manifest) => { manifest.checksums.unexpected = true; }],
        ["unknown install field", (manifest) => { manifest.install.unexpected = true; }],
        ["noncanonical install", (manifest) => { manifest.install.homebrew_tap.reverse(); }],
      ];
      for (const [label, mutate] of invalidManifests) {
        manifestMutator = mutate;
        refreshMetadata();
        const invalidManifest = await runInstaller({
          GITHUB_API_URL: apiRoot,
          GIT_SLOP_ACTION_VERSION: version,
          GIT_SLOP_RELEASE_REPOSITORY: "example/git-slop",
          RUNNER_TEMP: root,
        });
        assert.equal(invalidManifest.status, 1, `${label}: ${invalidManifest.stderr}`);
        assert.match(invalidManifest.stderr, /release-manifest\.json/u);
      }
      manifestMutator = null;
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
    "git-slop.1": ".TH GIT-SLOP 1\n",
  };
  for (const [name, contents] of Object.entries(payloads)) {
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
