import assert from "node:assert/strict";

export async function exerciseTagIdentity(context) {
  const { apiRoot, revision, root, runInstaller, tag, version } = context;
  context.releaseTagName = "v0.9.1";
  const rejectedTagMetadata = await runInstaller({
    GITHUB_API_URL: apiRoot,
    GIT_SLOP_ACTION_VERSION: version,
    GIT_SLOP_RELEASE_REPOSITORY: "example/git-slop",
    RUNNER_TEMP: root,
  });
  assert.equal(rejectedTagMetadata.status, 1);
  assert.match(rejectedTagMetadata.stderr, /inconsistent GitHub release metadata/u);
  context.releaseTagName = tag;

  context.servedTagRevision = "c".repeat(40);
  const rejectedTagRevision = await runInstaller({
    GITHUB_API_URL: apiRoot,
    GIT_SLOP_ACTION_VERSION: version,
    GIT_SLOP_RELEASE_REPOSITORY: "example/git-slop",
    RUNNER_TEMP: root,
  });
  assert.equal(rejectedTagRevision.status, 1);
  assert.match(rejectedTagRevision.stderr, /release tag .* resolves to/u);
  context.servedTagRevision = revision;

  context.tagReferenceType = "tag";
  const acceptedAnnotatedTag = await runInstaller({
    GITHUB_API_URL: apiRoot,
    GIT_SLOP_ACTION_VERSION: version,
    GIT_SLOP_RELEASE_REPOSITORY: "example/git-slop",
    RUNNER_TEMP: root,
  });
  assert.equal(acceptedAnnotatedTag.status, 0, acceptedAnnotatedTag.stderr);
  context.tagReferenceType = "commit";

  context.tagReferenceName = `refs/heads/${tag}`;
  const rejectedBranchAlias = await runInstaller({
    GITHUB_API_URL: apiRoot,
    GIT_SLOP_ACTION_VERSION: version,
    GIT_SLOP_RELEASE_REPOSITORY: "example/git-slop",
    RUNNER_TEMP: root,
  });
  assert.equal(rejectedBranchAlias.status, 1);
  assert.match(rejectedBranchAlias.stderr, /invalid exact-reference metadata/u);
  context.tagReferenceName = `refs/tags/${tag}`;
}
