# Installation

Git Slop is distributed as one native `git-slop` executable. Python is not
required. When the executable is on `PATH`, Git also accepts `git slop`.

## Homebrew

Homebrew remains the supported local install and upgrade path on macOS and
Linux:

```bash
brew tap coreycoto/tap
brew install coreycoto/tap/git-slop
git-slop version
```

Upgrade with:

```bash
brew update
brew upgrade coreycoto/tap/git-slop
git-slop version
```

The existing `coreycoto/tap/git-slop` formula name is stable across the Rust
migration, so an existing Homebrew installation upgrades in place.

## GitHub Release Archives

Each semver GitHub Release publishes checksummed archives for:

| Platform | Target | Archive |
| --- | --- | --- |
| Linux x86-64 | `x86_64-unknown-linux-gnu` | `.tar.gz` |
| Linux ARM64 | `aarch64-unknown-linux-gnu` | `.tar.gz` |
| macOS Intel | `x86_64-apple-darwin` | `.tar.gz` |
| macOS Apple Silicon | `aarch64-apple-darwin` | `.tar.gz` |
| Windows x86-64 | `x86_64-pc-windows-msvc` | `.zip` |

Archive names follow this stable contract:

```text
git-slop-v<version>-<target>.tar.gz
git-slop-v<version>-<target>.zip
```

Download the archive plus `SHA256SUMS`, verify the exact filename, then place
`git-slop` (`git-slop.exe` on Windows) on `PATH`. For example:

```bash
release=v0.9.0
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

Before the first crates.io publication is verified, install the tagged source
directly when a Cargo-based install is required:

```bash
cargo install \
  --git https://github.com/coreycoto/git-slop.git \
  --tag v0.9.0 \
  --locked
```

After `git-slop` `0.9.0` is confirmed on crates.io, this shorter form is also
supported:

```bash
cargo install git-slop --version 0.9.0 --locked
```

CI jobs should prefer the repository's GitHub Action or a checksummed prebuilt
archive so they do not spend time compiling Rust or installing Homebrew.

Contributor setup is documented in [Contributing](../CONTRIBUTING.md).
