---
name: adopt-repo
description: Add git-slop to a consumer repository using the canonical plugin, native CLI, and advisory GitHub Action contract.
---

# Adopt Git Slop In A Repository

Use this skill when a repository should start consuming `git-slop`.

## Adoption Contract

- Add `git-slop` Codex plugin source metadata alongside any existing shared
  workflow plugin sources.
- For local use, prefer a repo wrapper such as `./scripts/git_slop.sh` and
  require the native `git-slop` executable on `PATH`, usually from Homebrew.
- Pin the expected minimum CLI version in a repo-owned tool contract when the
  consumer needs an explicit version gate.
- Commit `.slop/config.yaml` when the repository intentionally configures Git
  Slop, and commit `.slop/.gitignore` so generated state stays untracked.
- Do not commit `.slop/latest/`, `.slop/runs/`, `.slop/cache/`, prompt packs,
  SARIF exports, plan JSON, or compare JSON as routine adoption output.
- In GitHub Actions, check out full history with `fetch-depth: 0`, then use the
  immutable `coreycoto/git-slop@v0.9.0` Action reference.
- Keep the Action at its safe defaults initially: advisory policy, at most 10
  annotations, `health.md`-only artifact, 14-day retention, and no pull request
  comment.
- Preserve the Action sequence: install the verified binary; run `find` once;
  append its generated `health.md` to the job summary; render bounded
  annotations with `health --format github`; upload allowlisted artifacts and
  optionally update one pull request comment; then run `check` only when
  `policy: enforce`. Advisory findings do not fail, but setup, analysis,
  rendering, or publication failures do.
- Opt into `artifact-contents: report` only for schema-4 automation. Do not
  upload `.slop/latest/` or `.slop/runs/` as broad directories.
- Keep `git-slop` observational until the repository explicitly promotes checks
  into required gates with `policy: enforce`.

Minimal CI adoption:

```yaml
permissions:
  contents: read

steps:
  - uses: actions/checkout@v7
    with:
      fetch-depth: 0
  - uses: coreycoto/git-slop@v0.9.0
```
