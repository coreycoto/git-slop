import assert from "node:assert/strict";

export async function exerciseReleaseManifest(context) {
  const { apiRoot, archiveBytes, digest, refreshMetadata, root, runInstaller, tag, version } = context;
  context.servedArchiveBytes = archiveBytes;
  context.servedArchiveDigest = digest;
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
    context.manifestMutator = mutate;
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
  context.manifestMutator = null;
}
