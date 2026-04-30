---
name: adopt-repo
description: Add git-slop to a consumer repository using the canonical plugin, Homebrew, uv release wheel, wrapper, and warn-only CI contract.
---

# Adopt Git Slop In A Repository

Use this skill when a repository should start consuming `git-slop`.

## Adoption Contract

- Add `git-slop-marketplace` source metadata alongside any existing shared
  workflow marketplace sources.
- Prefer a repo wrapper such as `./scripts/git_slop.sh`.
- Pin the expected release tag, wheel name, SHA256, and minimum CLI version in a
  repo-owned tool pin file.
- In CI, download the release wheel with `gh`, verify SHA256, install it
  with `uv tool install`, then run the repository's warn-only report lane.
- Keep `git-slop` observational until the repository explicitly promotes checks
  into required gates.
