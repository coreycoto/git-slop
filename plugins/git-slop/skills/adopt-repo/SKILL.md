---
name: adopt-repo
description: Integrate git-slop into a consumer repository through durable plugin source metadata, local configuration and wrapper conventions, and the advisory GitHub Action contract. Use when a user wants repository or CI adoption; use install-update instead when the outcome is only a native CLI installation.
---

# Adopt Git Slop In A Repository

Use this skill when a repository should start consuming `git-slop`.

## Adoption Contract

- Add `git-slop` Agent Plugin source metadata alongside any existing shared
  workflow plugin sources. For Codex consumers, retain the stable
  `git-slop-marketplace` distribution name while pinning a release that carries
  the root Agent Plugins 1.0.0 `plugin.json`.
- Treat the published crates.io package as the canonical source identity for a
  release. The Homebrew Formula installs that exact crate; a bottle is only its
  faster prebuilt transport. The public Action and external Scoop bucket install
  checksummed GitHub Release archives built from and verifiably bound to the
  same crate; Scoop publishes only after trusted Windows x64 and ARM64
  qualification.
- Verify that the intended version exists on crates.io and on the selected
  distribution surface before pinning it. Until publication is confirmed,
  describe that surface as pending rather than available.
- For local use, prefer a repo wrapper such as `./scripts/git_slop.sh` and
  require the native `git-slop` executable on `PATH`, usually from Homebrew on
  macOS/Linux or Scoop on Windows.
- Pin the expected minimum CLI version in a repo-owned tool contract when the
  consumer needs an explicit version gate.
- Verify installed release provenance with `git-slop build-info --format json`.
  Require the expected version and full source revision with
  `source_dirty: false`; `git-slop version` alone is not a provenance proof.
- Commit `.slop/config.yaml` when the repository intentionally configures Git
  Slop, and commit `.slop/.gitignore` so generated state stays untracked.
- Do not commit `.slop/latest/`, `.slop/runs/`, `.slop/cache/`, prompt packs,
  SARIF exports, plan JSON, or compare JSON as routine adoption output.
- After the requested crates.io package, GitHub Release, and Action tag are
  published, check out full history with `fetch-depth: 0`, then use the exact
  immutable `coreycoto/git-slop@v<version>` Action reference.
- Treat the Action's `source-revision`, `crate-sha256`, and
  `release-manifest-sha256` outputs as distribution evidence. Its installer also
  runs `build-info --format json` and rejects a binary that does not match the
  release manifest's version and source revision.
- Keep the Action at its safe defaults initially: advisory policy, at most 10
  annotations, `health.md`-only artifact, 14-day retention, and no pull request
  comment.
- Preserve the Action sequence: install the verified binary; run `find` once;
  append its generated `health.md` to the job summary; render bounded
  annotations with `health --format github`; upload allowlisted artifacts and
  optionally update one pull request comment; then run `check` only when
  `policy: enforce`. Advisory findings do not fail, but setup, analysis,
  rendering, or publication failures do.
- Opt into `artifact-contents: report` only for schema-5 automation. Do not
  upload `.slop/latest/` or `.slop/runs/` as broad directories.
- Keep `git-slop` observational until the repository explicitly promotes checks
  into required gates with `policy: enforce`.

Minimal CI adoption after `0.16.0` and its matching release assets are published:

```yaml
permissions:
  contents: read

steps:
  - uses: actions/checkout@v7
    with:
      fetch-depth: 0
  - uses: coreycoto/git-slop@v0.16.0
```
