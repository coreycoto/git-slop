import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
import { join } from "node:path";

export async function exerciseReleaseAssets(context) {
  const { apiRoot, archiveBytes, assetName, digest, formulaBytes, refreshMetadata, root, runInstaller, stageName, validateArchiveFormat, version } = context;
  const zipArchive = join(root, "zip-fixture.zip");
  const zipped = spawnSync("zip", ["-q", "-D", "-r", zipArchive, stageName], {
    cwd: join(root, "stage"),
    encoding: "utf8",
  });
  if (zipped.status === 0) {
    context.servedArchiveBytes = readFileSync(zipArchive);
    context.servedArchiveDigest = createHash("sha256").update(context.servedArchiveBytes).digest("hex");
    refreshMetadata();
    const wrongArchiveFormat = await runInstaller({
      GITHUB_API_URL: apiRoot,
      GIT_SLOP_ACTION_VERSION: version,
      GIT_SLOP_RELEASE_REPOSITORY: "example/git-slop",
      RUNNER_TEMP: root,
    });
    assert.equal(wrongArchiveFormat.status, 1);
    assert.match(wrongArchiveFormat.stderr, /required tar\.gz format/u);

    validateArchiveFormat(context.servedArchiveBytes, "zip");
    assert.throws(() => validateArchiveFormat(context.servedArchiveBytes, "tar.gz"), /tar\.gz/u);
    assert.throws(() => validateArchiveFormat(archiveBytes, "zip"), /ZIP/u);
  }

  context.servedArchiveBytes = archiveBytes;
  context.servedArchiveDigest = digest;
  refreshMetadata();
  context.checksumBytes = Buffer.concat([
    context.checksumBytes,
    Buffer.from(`${context.servedArchiveDigest}  ${assetName}\n`, "utf8"),
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
    ["missing", (assets) => assets.pop(), /exactly 12/u],
    [
      "extra",
      (assets) => assets.push({ ...assets[0], name: "unexpected.bin" }),
      /exactly 12/u,
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
      "SBOM digest",
      (assets) => {
        assets.find((entry) => entry.name === "git-slop.cdx.json").digest =
          `sha256:${"9".repeat(64)}`;
      },
      /does not authenticate git-slop\.cdx\.json/u,
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
    context.assetMutator = mutate;
    const invalidAssets = await runInstaller({
      GITHUB_API_URL: apiRoot,
      GIT_SLOP_ACTION_VERSION: version,
      GIT_SLOP_RELEASE_REPOSITORY: "example/git-slop",
      RUNNER_TEMP: root,
    });
    assert.equal(invalidAssets.status, 1, `${label}: ${invalidAssets.stderr}`);
    assert.match(invalidAssets.stderr, message);
  }
  context.assetMutator = null;

  context.servedFormulaBytes = Buffer.concat([formulaBytes, Buffer.from("unexpected")]);
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
  context.servedFormulaBytes = formulaBytes;

  const formulaDigest = createHash("sha256").update(formulaBytes).digest("hex");
  context.checksumBytes = Buffer.from(
    context.checksumBytes.toString("utf8").replace(formulaDigest, "e".repeat(64)),
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
}
