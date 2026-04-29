---
name: install-update
description: Install or update the git-slop CLI from a private GitHub release wheel or the private Homebrew tap. Use when a repository or machine needs a usable git-slop binary before running reports.
---

# Install Or Update Git Slop

Use this skill when the user needs `git-slop` installed or updated.

## Preferred Paths

- Homebrew on macOS:
  - `brew tap coreycoto/tap git@github.com:coreycoto/homebrew-tap.git`
  - `brew install coreycoto/tap/git-slop`
  - `brew upgrade coreycoto/tap/git-slop`
- Private release wheel with `uv`:
  - `gh release download <tag> --repo coreycoto/git-slop --pattern 'git_slop-*.whl' --dir .artifacts/git-slop`
  - verify the SHA256 from the release manifest
  - `uv tool install --force .artifacts/git-slop/<wheel>`

## Verification

Run `git-slop version`. The output must match the repository's pinned minimum
version when a consumer repo defines one.
