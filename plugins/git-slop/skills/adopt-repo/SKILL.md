---
name: adopt-repo
description: Add git-slop to a consumer repository using the canonical plugin, Homebrew, wrapper, and warn-only CI contract.
---

# Adopt Git Slop In A Repository

Use this skill when a repository should start consuming `git-slop`.

## Adoption Contract

- Add `git-slop-marketplace` source metadata alongside any existing shared
  workflow marketplace sources.
- Prefer a repo wrapper such as `./scripts/git_slop.sh`.
- Require `git-slop` on `PATH`, usually installed from Homebrew.
- Pin the expected minimum CLI version in a repo-owned tool contract when the
  consumer needs an explicit version gate.
- Commit `.slop/config.yaml` when the repository intentionally configures Git
  Slop, and commit `.slop/.gitignore` so generated state stays untracked.
- Do not commit `.slop/latest/`, `.slop/runs/`, `.slop/cache/`, prompt packs,
  SARIF exports, plan JSON, or compare JSON as routine adoption output.
- In CI, install or provide `git-slop` before running the repository's warn-only
  report lane.
- Keep `git-slop` observational until the repository explicitly promotes checks
  into required gates.
