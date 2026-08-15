# Installation

Git Slop is distributed as one `git-slop` executable. When it is on `PATH`, Git
also accepts `git slop`.

The examples below pin 0.15.0. Confirm that the requested distribution surface
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

Uninstall with `brew uninstall coreycoto/tap/git-slop`; remove the tap with
`brew untap coreycoto/tap` only when no other formula from it is needed.

The existing `coreycoto/tap/git-slop` formula name is stable across the Rust
migration, so an existing Homebrew installation upgrades in place. The Formula
generates and installs Bash, Zsh, and Fish completions from the installed
executable.

The tap publishes bottles for supported macOS and Linux targets. Homebrew uses
a matching bottle when one is available and falls back to the Formula's exact,
SHA-256-pinned `git-slop-<version>.crate` source build on other supported
systems.

## Cargo Binstall

The crate publishes [Cargo Binstall](https://github.com/cargo-bins/cargo-binstall)
metadata for the checksummed native release
archives. After the exact crate and GitHub Release are public, install the
pinned version without compiling it locally:

```bash
cargo binstall git-slop@0.15.0
git-slop version
git-slop build-info --format json
```

For unattended CI, add `--no-confirm`. Binstall resolves metadata through the
published crate and downloads the matching GitHub Release archive. It is a
transport convenience, not a separate source identity; continue to verify
`build-info` and the requested version. If no supported archive matches,
Binstall may offer a source-build fallback, which should be treated as a
different installation path.

Update to another explicitly reviewed version by changing the pin and using
`--force`; uninstall through Cargo's shared binary registry:

```bash
cargo binstall --force git-slop@0.15.0
cargo uninstall git-slop
```

## Scoop

Scoop is the supported Windows package-manager path beginning with 0.9.5. The
manifest lives in the separate, public
[`coreycoto/scoop-bucket`](https://github.com/coreycoto/scoop-bucket)
repository and consumes the existing Windows release archives; it is not an
additional `git-slop` release asset.

After the bucket lists 0.15.0, install it from PowerShell:

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
| macOS Intel | `x86_64-apple-darwin` | `.tar.gz` |
| Static Linux x86-64 (Alpine/minimal containers) | `x86_64-unknown-linux-musl` | `.tar.gz` |
| Windows x86-64 | `x86_64-pc-windows-msvc` | `.zip` |
| Windows ARM64 | `aarch64-pc-windows-msvc` | `.zip` |

Archive names follow this stable contract:

```text
git-slop-v<version>-<target>.tar.gz
git-slop-v<version>-<target>.zip
```

Use the [Unix archive guide](archive-install.md) or the
[Windows PowerShell guide](archive-install-windows.md) to verify the immutable
release, manifest identity, checksum, size, attestation, and installed build
before trusting a direct installation.

Release automation also publishes `release-manifest.json`, which maps every
target to its URL, size, and SHA-256 digest for setup actions and other
automated consumers. Its published contract is
[`schemas/release-manifest-3.json`](../schemas/release-manifest-3.json) and is
also available from `git slop schema release-manifest`.

Verify the release record itself before downloading assets:

```bash
gh release verify v0.15.0 --repo coreycoto/git-slop
```

The direct-install guides also cover safe updates, shell activation, and
uninstallation without removing unrelated user configuration.

## Shell Completions

Native archives include generated completion sources under `completions/` for
Bash, Zsh, Fish, PowerShell, and Nushell. Homebrew installs Bash, Zsh, and Fish
completions automatically. A Scoop installation retains all five sources under
its version directory; for the current PowerShell session:

```powershell
. (Join-Path (scoop prefix git-slop) "completions/git-slop.powershell")
```

For a direct install, generate from the live command tree so the completion
contract matches the binary:

```bash
git-slop completions bash > ~/.local/share/bash-completion/completions/git-slop
git-slop completions zsh > ~/.zfunc/_git-slop
git-slop completions fish > ~/.config/fish/completions/git-slop.fish
```

PowerShell and Nushell use `git-slop completions powershell` and
`git-slop completions nushell` respectively.

GitHub also publishes artifact attestations for every native archive. The
schema-3 manifest contains these exact commands; for `v0.15.0` they are:

```bash
gh attestation verify git-slop-v0.15.0-aarch64-apple-darwin.tar.gz --repo coreycoto/git-slop
gh attestation verify git-slop-v0.15.0-aarch64-pc-windows-msvc.zip --repo coreycoto/git-slop
gh attestation verify git-slop-v0.15.0-aarch64-unknown-linux-gnu.tar.gz --repo coreycoto/git-slop
gh attestation verify git-slop-v0.15.0-x86_64-apple-darwin.tar.gz --repo coreycoto/git-slop
gh attestation verify git-slop-v0.15.0-x86_64-pc-windows-msvc.zip --repo coreycoto/git-slop
gh attestation verify git-slop-v0.15.0-x86_64-unknown-linux-gnu.tar.gz --repo coreycoto/git-slop
gh attestation verify git-slop-v0.15.0-x86_64-unknown-linux-musl.tar.gz --repo coreycoto/git-slop
```

Release tags through `v0.10.1` are lightweight tags whose target commit is
GitHub-signed. Starting with `v0.11.0`, releases use annotated OpenPGP-signed
tags. Pin the release-signing primary-key fingerprint before trusting a tag:

```bash
expected_fingerprint=62AC6FBF75A48D27E24083C733212861C6351839
key_file="$(mktemp)"
trap 'rm -f "$key_file"' EXIT
curl --fail --silent --show-error --location https://github.com/coreycoto.gpg >"$key_file"
gpg --batch --with-colons --show-keys "$key_file" \
  | awk -F: '$1 == "fpr" { print $10 }' \
  | grep -Fx "$expected_fingerprint"
gpg --batch --import "$key_file"
git verify-tag "${release}"
```

The fingerprint, not an email address or short key ID, is the trust anchor.
Continue to verify the release manifest, checksums, SBOMs, artifact
attestations, and installed binary `build-info` identity as separate provenance
layers; the [release trust graph](release-trust.md) defines their exact order
and explains why the manifest is not a circular root.

## Cargo

Install the canonical crates.io package directly:

```bash
cargo install git-slop --version 0.15.0 --locked
git-slop build-info --format json
```

Update and uninstall explicitly:

```bash
cargo install git-slop --version 0.15.0 --locked --force
cargo uninstall git-slop
```

For a verified 0.15.0 release, `source_revision` is the full commit named by
`v0.15.0` and `source_dirty` is `false`. A local source build can report `null`
for provenance it cannot prove; that is not equivalent to a release build.

CI jobs should prefer the repository's GitHub Action or a checksummed prebuilt
archive so they do not spend time compiling Rust or installing Homebrew. Those
prebuilt archives are produced from the exact crates.io package and record its
digest and full source revision in `release-manifest.json`.

Contributor setup is documented in [Contributing](../CONTRIBUTING.md).
