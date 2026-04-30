---
name: install-update
description: Install or update the git-slop CLI from the Homebrew tap. Use when a repository or machine needs a usable git-slop binary before running reports.
---

# Install Or Update Git Slop

Use this skill when the user needs `git-slop` installed or updated.

## Preferred Paths

- Homebrew:
  - `brew tap coreycoto/tap`
  - `brew install coreycoto/tap/git-slop`
  - `brew upgrade coreycoto/tap/git-slop`

## Verification

Run `git-slop version`. The output must match the repository's pinned minimum
version when a consumer repo defines one.
