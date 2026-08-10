import assert from "node:assert/strict";
import { join } from "node:path";

export async function exerciseDraftRelease(context) {
  const { apiRoot, assetName, draftDownloadBase, draftReleaseId, draftReleaseSlug, outputs, root, runInstaller, version } = context;
  context.releaseDraft = true;
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
  assert.equal(context.releaseIdRequests, 1);

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
    context.releaseAssetDownloadBaseOverride = downloadBase;
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
  context.releaseAssetDownloadBaseOverride = null;

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

  context.servedReleaseId = draftReleaseId + 1;
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
  context.servedReleaseId = draftReleaseId;

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

  context.releaseDraft = false;
  context.releaseAssetDownloadBaseOverride = draftDownloadBase;
  const rejectedPublishedDraftAssetUrl = await runInstaller({
    GITHUB_API_URL: apiRoot,
    GIT_SLOP_ACTION_VERSION: version,
    GIT_SLOP_RELEASE_REPOSITORY: "example/git-slop",
    RUNNER_TEMP: root,
  });
  assert.equal(rejectedPublishedDraftAssetUrl.status, 1);
  assert.match(rejectedPublishedDraftAssetUrl.stderr, /invalid metadata for asset/u);
  context.releaseAssetDownloadBaseOverride = null;
}
