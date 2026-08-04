---
name: install-update
description: Install, update, and verify the native git-slop CLI through its crates.io-backed Cargo, Homebrew, or GitHub Release distribution paths. Use when a repository or machine needs a usable binary or when an installed binary's version and source revision must be proven before running reports.
---

# Install Or Update Git Slop

Use this skill when the user needs `git-slop` installed or updated.

## Availability Gate

Treat a version as available only after its requested distribution surface is
published and verified:

- Cargo installation requires the exact version on crates.io.
- Homebrew installation requires the crates.io package plus the matching tap
  formula; a bottle may provide the fast path after it is published.
- The GitHub Action requires the crates.io package, matching GitHub Release
  archives and manifest, and the immutable Action tag.

Until those surfaces exist, describe `0.9.0` as prepared or pending. Do not tell
users that `cargo install git-slop --version 0.9.0`,
`brew upgrade coreycoto/tap/git-slop`, or `coreycoto/git-slop@v0.9.0` is
available without verifying the corresponding publication.

## Distribution Contract

- Treat the published `git-slop` crate on crates.io as the canonical source
  identity for the release.
- The Homebrew Formula identifies that exact `.crate` by crates.io URL and
  SHA-256 and installs it with Cargo. A Homebrew bottle is only a faster,
  prebuilt transport of that Formula; it is not a separate source identity.
- The public GitHub Action installs a prebuilt GitHub Release archive built
  from, and verifiably bound to, the same crate. Its installer verifies the
  release tag, release manifest, crate checksum, archive checksum, and installed
  binary provenance before analysis begins.

## Install Paths

- Cargo, after confirming the requested version on crates.io:
  - `cargo install git-slop --version <version> --locked`
- Homebrew, after confirming the matching tap Formula is published:
  - `brew tap coreycoto/tap`
  - `brew install coreycoto/tap/git-slop`
  - `brew upgrade coreycoto/tap/git-slop`
- GitHub Actions, after confirming the matching immutable tag and release:
  - `uses: coreycoto/git-slop@v<version>`

The public CLI is a native Rust executable. Do not add alternate runtime
dependencies.

## Verification

Run both:

```bash
git-slop version
git-slop build-info --format json
```

The version must match the repository's pinned contract. For a published
release, build-info schema 1 must report `project: "git-slop"`, the same version,
the full canonical `source_revision`, and `source_dirty: false`. Reject a
release install with a missing or mismatched revision or a dirty source marker;
the version string alone does not prove source identity.
