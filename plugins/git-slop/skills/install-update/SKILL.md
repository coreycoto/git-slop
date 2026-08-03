---
name: install-update
description: Install or update the native git-slop CLI from the Homebrew tap. Use when a repository or machine needs a usable git-slop binary before running reports.
---

# Install Or Update Git Slop

Use this skill when the user needs `git-slop` installed or updated.

## Preferred Paths

- Homebrew:
  - `brew tap coreycoto/tap`
  - `brew install coreycoto/tap/git-slop`
  - `brew upgrade coreycoto/tap/git-slop`

The public CLI is a native Rust executable. Do not add Python as a runtime
dependency. In GitHub Actions, prefer `coreycoto/git-slop@v0.9.0`, which
downloads and verifies the matching prebuilt release archive.

## Verification

Run `git-slop version`. The output must match the repository's pinned minimum
version when a consumer repo defines one. Do not claim `cargo install git-slop`
is available until the requested version is verified on crates.io; tagged
source installs and the Homebrew/release paths remain distinct.
