---
name: install-update
description: Install, update, and verify the native git-slop CLI through Cargo, Homebrew, Scoop, or a checksummed release archive. Use when a machine needs a usable binary on PATH or an installed binary's version and source revision must be proven; repository configuration and GitHub Action adoption belong to the adopt-repo skill.
---

# Install Or Update Git Slop

Use this skill when the user needs `git-slop` installed or updated.

## Availability Gate

Treat a version as available only after its requested distribution surface is
published and verified:

- Cargo installation requires the exact version on crates.io.
- Homebrew installation requires the crates.io package plus the matching tap
  formula; a bottle may provide the fast path after it is published.
- Scoop installation requires the matching public GitHub Release plus the
  external bucket manifest after native Windows x64 and ARM64 qualification.
- Archive installation requires the exact GitHub Release archive, its entry in
  `SHA256SUMS`, and the matching `release-manifest.json` target identity.

Do not infer availability from a source tag, documentation, or another
distribution surface. Verify the exact crates.io version, tap Formula, or Scoop
manifest before recommending its corresponding install command.

## Distribution Contract

- Treat the published `git-slop` crate on crates.io as the canonical source
  identity for the release.
- The Homebrew Formula identifies that exact `.crate` by crates.io URL and
  SHA-256 and installs it with Cargo. A Homebrew bottle is only a faster,
  prebuilt transport of that Formula; it is not a separate source identity.
- The external Scoop manifest selects a checksummed Windows GitHub Release
  archive whose hash comes from the authoritative `SHA256SUMS`; trusted bucket
  automation publishes it only after exact-release and native qualification.

## Install Paths

- Cargo, after confirming the requested version on crates.io:
  - `cargo install git-slop --version <version> --locked`
- Homebrew, after confirming the matching tap Formula is published:
  - `brew tap coreycoto/tap`
  - `brew install coreycoto/tap/git-slop`
  - `brew upgrade coreycoto/tap/git-slop`
- Scoop, after confirming the external bucket manifest is published:
  - `scoop bucket add coreycoto https://github.com/coreycoto/scoop-bucket`
  - `scoop install coreycoto/git-slop`
  - `scoop update git-slop`
- Release archive, when no package manager is available:
  - download `SHA256SUMS`, `release-manifest.json`, and the exact platform archive
  - verify the archive SHA-256 against both files
  - extract Unix archives with `tar --no-same-owner -xzf <archive>`
  - install `git-slop` and the bundled `man/git-slop.1` into user-owned paths

The public CLI is a native Rust executable. Do not add alternate runtime
dependencies. Use the separate `adopt-repo` skill when the requested outcome
is durable repository configuration or GitHub Action integration.

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
the version string alone does not prove source identity. For Scoop, also run
`git slop version` to prove the Git subcommand resolves through the installed
shim.
