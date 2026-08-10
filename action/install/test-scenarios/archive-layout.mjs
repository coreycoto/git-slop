import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { readFileSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";

export async function exerciseArchiveLayout(context) {
  const { apiRoot, assetName, createArchive, refreshMetadata, root, runInstaller, stage, stageName, version } = context;
  writeFileSync(join(stage, "UNEXPECTED"), "unsafe extra member\n", "utf8");
  const unsafeArchive = join(root, `unsafe-${assetName}`);
  createArchive(
    unsafeArchive,
    ["-c", "-z"],
    ["-C", join(root, "stage"), stageName],
  );
  context.servedArchiveBytes = readFileSync(unsafeArchive);
  context.servedArchiveDigest = createHash("sha256").update(context.servedArchiveBytes).digest("hex");
  refreshMetadata();
  const unsafeInventory = await runInstaller({
    GITHUB_API_URL: apiRoot,
    GIT_SLOP_ACTION_VERSION: version,
    GIT_SLOP_RELEASE_REPOSITORY: "example/git-slop",
    RUNNER_TEMP: root,
  });
  assert.equal(unsafeInventory.status, 1);
  assert.match(unsafeInventory.stderr, /exact Git Slop release layout/u);

  rmSync(join(stage, "UNEXPECTED"));
  rmSync(join(stage, "completions", "git-slop.nushell"));
  const missingCompletionArchive = join(root, `missing-completion-${assetName}`);
  createArchive(
    missingCompletionArchive,
    ["-c", "-z"],
    ["-C", join(root, "stage"), stageName],
  );
  context.servedArchiveBytes = readFileSync(missingCompletionArchive);
  context.servedArchiveDigest = createHash("sha256").update(context.servedArchiveBytes).digest("hex");
  refreshMetadata();
  const missingCompletion = await runInstaller({
    GITHUB_API_URL: apiRoot,
    GIT_SLOP_ACTION_VERSION: version,
    GIT_SLOP_RELEASE_REPOSITORY: "example/git-slop",
    RUNNER_TEMP: root,
  });
  assert.equal(missingCompletion.status, 1);
  assert.match(missingCompletion.stderr, /exact Git Slop release layout/u);

  writeFileSync(
    join(stage, "completions", "git-slop.nushell"),
    "# nushell completion fixture\n",
    "utf8",
  );
  writeFileSync(join(stage, "schemas", "unversioned.json"), "{}\n", "utf8");
  const unversionedSchemaArchive = join(root, `unversioned-schema-${assetName}`);
  createArchive(
    unversionedSchemaArchive,
    ["-c", "-z"],
    ["-C", join(root, "stage"), stageName],
  );
  context.servedArchiveBytes = readFileSync(unversionedSchemaArchive);
  context.servedArchiveDigest = createHash("sha256").update(context.servedArchiveBytes).digest("hex");
  refreshMetadata();
  const unversionedSchema = await runInstaller({
    GITHUB_API_URL: apiRoot,
    GIT_SLOP_ACTION_VERSION: version,
    GIT_SLOP_RELEASE_REPOSITORY: "example/git-slop",
    RUNNER_TEMP: root,
  });
  assert.equal(unversionedSchema.status, 1);
  assert.match(unversionedSchema.stderr, /exact Git Slop release layout/u);
}
