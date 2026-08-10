import assert from "node:assert/strict";
import { existsSync, readFileSync } from "node:fs";
import { dirname, join } from "node:path";

export async function exerciseInstallAndCache(context) {
  const { apiRoot, assetName, digest, githubPath, output, outputs, publishedDownloadBase, revision, root, runInstaller, target, version } = context;
  const toolCache = join(root, "tool-cache");
  const installed = await runInstaller({
    GITHUB_API_URL: apiRoot,
    GITHUB_OUTPUT: output,
    GITHUB_PATH: githubPath,
    GIT_SLOP_ACTION_VERSION: version,
    GIT_SLOP_GITHUB_TOKEN: "github-test-token",
    GIT_SLOP_RELEASE_REPOSITORY: "example/git-slop",
    RUNNER_TEMP: root,
    RUNNER_TOOL_CACHE: toolCache,
  });
  assert.equal(installed.status, 0, installed.stderr);
  const actual = outputs(output);
  assert.equal(actual.version, version);
  assert.equal(actual.target, target);
  assert.equal(actual.asset, assetName);
  assert.equal(actual["asset-url"], `${publishedDownloadBase}/${assetName}`);
  assert.equal(actual.sha256, digest);
  assert.equal(actual["source-revision"], revision);
  assert.equal(actual["crate-sha256"], context.crateSha256);
  assert.equal(actual["cache-hit"], "false");
  assert.match(actual["release-manifest-sha256"], /^[a-f0-9]{64}$/u);
  assert.ok(existsSync(actual["binary-path"]));
  assert.equal(readFileSync(githubPath, "utf8").trim(), dirname(actual["binary-path"]));
  assert.equal(context.crateAuthorizationObserved, null, "GitHub token leaked to crates.io fetch");
  assert.equal(context.releaseTagRequests, 1);
  assert.equal(context.releaseIdRequests, 0);
  assert.equal(context.archiveDownloadRequests, 1);
  assert.equal(context.crateDownloadRequests, 1);

  const cachedOutput = join(root, "github-cache-output.txt");
  const cachedInstall = await runInstaller({
    GITHUB_API_URL: apiRoot,
    GITHUB_OUTPUT: cachedOutput,
    GIT_SLOP_ACTION_VERSION: version,
    GIT_SLOP_GITHUB_TOKEN: "github-test-token",
    GIT_SLOP_RELEASE_REPOSITORY: "example/git-slop",
    RUNNER_TEMP: root,
    RUNNER_TOOL_CACHE: toolCache,
  });
  assert.equal(cachedInstall.status, 0, cachedInstall.stderr);
  const cached = outputs(cachedOutput);
  assert.equal(cached["cache-hit"], "true");
  assert.equal(cached["binary-path"], actual["binary-path"]);
  assert.equal(context.archiveDownloadRequests, 1, "cache hit downloaded the archive again");
  assert.equal(context.crateDownloadRequests, 1, "cache hit downloaded the crate again");
}
