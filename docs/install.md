# Installation

Git Slop is distributed as one `git-slop` executable. When it is on `PATH`, Git
also accepts `git slop`.

The examples below pin 0.9.5. Confirm that the requested distribution surface
publishes that exact version before installing it; documentation on `main` does
not itself prove availability.

## Homebrew

Homebrew remains the supported local install and upgrade path on macOS and
Linux:

```bash
brew tap coreycoto/tap
brew install coreycoto/tap/git-slop
git-slop version
git-slop build-info --format json
```

Upgrade with:

```bash
brew update
brew upgrade coreycoto/tap/git-slop
git-slop version
git-slop build-info --format json
```

The existing `coreycoto/tap/git-slop` formula name is stable across the Rust
migration, so an existing Homebrew installation upgrades in place.

The Formula downloads the exact `git-slop-<version>.crate` file from
`static.crates.io`, verifies its SHA-256, and builds it locally with Rust rather
than installing a GitHub archive.

## Scoop

Scoop is the supported Windows package-manager path beginning with 0.9.5. The
manifest lives in the separate, public
[`coreycoto/scoop-bucket`](https://github.com/coreycoto/scoop-bucket)
repository and consumes the existing Windows release archives; it is not an
additional `git-slop` release asset.

After the bucket lists 0.9.5, install it from PowerShell:

```powershell
scoop bucket add coreycoto https://github.com/coreycoto/scoop-bucket
scoop install coreycoto/git-slop
git-slop version
git-slop build-info --format json
git slop version
```

Upgrade the existing installation in place:

```powershell
scoop update
scoop update git-slop
git-slop version
git-slop build-info --format json
git slop version
```

Uninstall with `scoop uninstall git-slop`. The manifest selects the x86-64 or
ARM64 ZIP for the host, verifies its SHA-256 from the release's authoritative
`SHA256SUMS`, extracts the versioned target directory, and exposes
`git-slop.exe` through a Scoop shim. Git then discovers the same shim as the
`git slop` subcommand. Scoop publication happens only after the stable GitHub
Release is public. The public-release verifier dispatches only the immutable
version, release ID, revision, and manifest digest; trusted bucket `main`
reverifies the release, creates a manifest-only PR, runs native Windows x64 and
ARM64 qualification, and ruleset-merges it without another release-environment
approval or per-release maintainer action.

## GitHub Release Archives

Each semver GitHub Release publishes checksummed archives for:

| Platform | Target | Archive |
| --- | --- | --- |
| Linux x86-64 | `x86_64-unknown-linux-gnu` | `.tar.gz` |
| Linux ARM64 | `aarch64-unknown-linux-gnu` | `.tar.gz` |
| macOS Apple Silicon | `aarch64-apple-darwin` | `.tar.gz` |
| Windows x86-64 | `x86_64-pc-windows-msvc` | `.zip` |
| Windows ARM64 | `aarch64-pc-windows-msvc` | `.zip` |

Archive names follow this stable contract:

```text
git-slop-v<version>-<target>.tar.gz
git-slop-v<version>-<target>.zip
```

Download the archive plus `SHA256SUMS`, verify the exact filename, then place
`git-slop` (`git-slop.exe` on Windows) on `PATH`. For example:

```bash
release=v0.9.5
target=x86_64-unknown-linux-gnu
gh release download "$release" \
  --repo coreycoto/git-slop \
  --pattern "git-slop-${release}-${target}.tar.gz" \
  --pattern SHA256SUMS
sha256sum --check SHA256SUMS --ignore-missing
tar -xzf "git-slop-${release}-${target}.tar.gz"
```

macOS users can select the downloaded archive's line from the same GNU-format
checksum file and verify it with `shasum`:

```bash
grep "  git-slop-${release}-${target}.tar.gz$" SHA256SUMS |
  shasum -a 256 -c -
```

Release automation also publishes `release-manifest.json`, which maps every
target to its URL, size, and SHA-256 digest for setup actions and other
automated consumers.

## Cargo

Install the canonical crates.io package directly:

```bash
cargo install git-slop --version 0.9.5 --locked
git-slop build-info --format json
```

For a verified 0.9.5 release, `source_revision` is the full commit named by
`v0.9.5` and `source_dirty` is `false`. A local source build can report `null`
for provenance it cannot prove; that is not equivalent to a release build.

CI jobs should prefer the repository's GitHub Action or a checksummed prebuilt
archive so they do not spend time compiling Rust or installing Homebrew. Those
prebuilt archives are produced from the exact crates.io package and record its
digest and full source revision in `release-manifest.json`.

Contributor setup is documented in [Contributing](../CONTRIBUTING.md).
