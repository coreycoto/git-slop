import assert from "node:assert/strict";
import { createHash } from "node:crypto";

export async function exerciseCrateSource(context) {
  const { apiRoot, buildCrateBytes, canonicalCrateBytes, canonicalCrateSha256, refreshMetadata, revision, root, runInstaller, version } = context;
  context.servedCrateBytes = Buffer.concat([canonicalCrateBytes, Buffer.from("unexpected")]);
  const rejectedCrateDigest = await runInstaller({
    GITHUB_API_URL: apiRoot,
    GIT_SLOP_ACTION_VERSION: version,
    GIT_SLOP_RELEASE_REPOSITORY: "example/git-slop",
    RUNNER_TEMP: root,
  });
  assert.equal(rejectedCrateDigest.status, 1);
  assert.match(rejectedCrateDigest.stderr, /package SHA-256 mismatch/u);

  context.servedCrateBytes = canonicalCrateBytes;
  context.crateContentLengthOverride = 16 * 1024 * 1024 + 1;
  const rejectedOversizedCrate = await runInstaller({
    GITHUB_API_URL: apiRoot,
    GIT_SLOP_ACTION_VERSION: version,
    GIT_SLOP_RELEASE_REPOSITORY: "example/git-slop",
    RUNNER_TEMP: root,
  });
  assert.equal(rejectedOversizedCrate.status, 1);
  assert.match(rejectedOversizedCrate.stderr, /exceeds its download size limit/u);
  context.crateContentLengthOverride = null;

  context.servedCrateBytes = buildCrateBytes("c".repeat(40));
  context.crateSha256 = createHash("sha256").update(context.servedCrateBytes).digest("hex");
  refreshMetadata();
  const rejectedCrateRevision = await runInstaller({
    GITHUB_API_URL: apiRoot,
    GIT_SLOP_ACTION_VERSION: version,
    GIT_SLOP_RELEASE_REPOSITORY: "example/git-slop",
    RUNNER_TEMP: root,
  });
  assert.equal(rejectedCrateRevision.status, 1);
  assert.match(rejectedCrateRevision.stderr, /VCS metadata does not match/u);

  context.servedCrateBytes = buildCrateBytes(revision, true);
  context.crateSha256 = createHash("sha256").update(context.servedCrateBytes).digest("hex");
  refreshMetadata();
  const rejectedDirtyCrate = await runInstaller({
    GITHUB_API_URL: apiRoot,
    GIT_SLOP_ACTION_VERSION: version,
    GIT_SLOP_RELEASE_REPOSITORY: "example/git-slop",
    RUNNER_TEMP: root,
  });
  assert.equal(rejectedDirtyCrate.status, 1);
  assert.match(rejectedDirtyCrate.stderr, /VCS metadata does not match/u);

  context.servedCrateBytes = buildCrateBytes(revision, false, true);
  context.crateSha256 = createHash("sha256").update(context.servedCrateBytes).digest("hex");
  refreshMetadata();
  const rejectedCrateRootEscape = await runInstaller({
    GITHUB_API_URL: apiRoot,
    GIT_SLOP_ACTION_VERSION: version,
    GIT_SLOP_RELEASE_REPOSITORY: "example/git-slop",
    RUNNER_TEMP: root,
  });
  assert.equal(rejectedCrateRootEscape.status, 1);
  assert.match(rejectedCrateRootEscape.stderr, /escapes the expected root/u);

  context.servedCrateBytes = canonicalCrateBytes;
  context.crateSha256 = canonicalCrateSha256;
  refreshMetadata();

  const rejectedNoncanonicalVersion = await runInstaller({
    GITHUB_API_URL: apiRoot,
    GIT_SLOP_ACTION_VERSION: "01.2.3",
    GIT_SLOP_RELEASE_REPOSITORY: "example/git-slop",
    RUNNER_TEMP: root,
  });
  assert.equal(rejectedNoncanonicalVersion.status, 1);
  assert.match(rejectedNoncanonicalVersion.stderr, /invalid release version/u);
}
