# Git Slop Release Checklist

Use this checklist for semver releases that publish the Rust CLI, update the
Codex plugin install contract, and preserve existing Homebrew upgrades.

## Prepare

- Confirm `Cargo.toml` contains the intended version and `Cargo.lock` is current.
- Confirm the release tag does not already exist.
- Confirm CLI flags, exit codes, `.slop/` layout, report JSON, SARIF, and
  deterministic fixture output remain compatible with the prior release.
- Run the release validation locally:

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features --locked -- -D warnings
cargo test --all-targets --all-features --locked
cargo package --locked
cargo publish --dry-run --locked
```

`cargo publish --dry-run` is required even when the release will not publish to
crates.io. It catches registry package omissions without needing credentials.

## Prepare The Tag And Homebrew Formula

Before changing the tap formula, prepare the upgrade lane by installing the
currently published Python-backed version on a machine that will remain intact
until the new formula is published:

```bash
brew update
brew install coreycoto/tap/git-slop
git-slop version
brew list --versions git-slop
```

Record that output in the release evidence. It must identify the prior version;
installing the new version and then using `brew reinstall` is not an upgrade
test.

Create the local semver tag at the exact release commit, then run the helper.
It resolves only `refs/tags/v<version>^{commit}`, rechecks the Rust package, and
rewrites the adjacent Homebrew tap formula as a native Rust source build with
the exact tag and revision:

```bash
git tag v<version>
python3 scripts/release_prepare.py --version <version> --tap ../homebrew-tap
```

Do not commit or merge the tap update until the GitHub Release assets exist.

## Publish GitHub Release Assets

Push the tag:

```bash
git push origin v<version>
```

`.github/workflows/release-publish.yml` must build and smoke the packaged
binary—not only `target/release`—for:

- `x86_64-unknown-linux-gnu`
- `aarch64-unknown-linux-gnu`
- `x86_64-apple-darwin`
- `aarch64-apple-darwin`
- `x86_64-pc-windows-msvc`

The GitHub Release is incomplete until it contains all five archives plus:

- `SHA256SUMS`, emitted deterministically in GNU `sha256sum` format
- `release-manifest.json`, schema version 2, with target, URL, SHA-256, and size
  for every archive; its top-level version, tag, and revision must agree with
  `homebrew_source`
- GitHub build-provenance attestations for the published assets

Release workflow reruns may replace assets only while the release is a draft.
If the tag already has a published release, the workflow verifies the exact
five-archive asset set, checksums, regenerated manifest, and Action installer,
then exits without mutating the release. Never use `--clobber` against a
published release.

Verify:

```bash
gh run list \
  --repo coreycoto/git-slop \
  --workflow release-publish.yml \
  --limit 1
gh release view v<version> \
  --repo coreycoto/git-slop \
  --json url,tagName,assets
```

Download the release into a clean directory and run `sha256sum --check
SHA256SUMS` before testing at least one archive.

## Publish crates.io

The automated release performs a crates.io dry-run only. The first real
publication requires a crates.io owner credential and must be performed
deliberately from the exact tagged commit:

```bash
cargo publish --locked
```

Use a Cargo credential provider or a short-lived, narrowly scoped token; never
commit or print the token. After the first version exists, configure crates.io
Trusted Publishing for `coreycoto/git-slop` and the exact release workflow.
Only then add an OIDC publish job with `id-token: write` and a protected release
environment.

Do not advertise `cargo install git-slop` until the published version is
confirmed through crates.io.

## Update Homebrew

In `coreycoto/homebrew-tap`, review the generated formula and use the existing
`brew test-bot` / `brew pr-pull` bottle workflow.

First, complete the upgrade lane on the machine where the prior Python-backed
formula was installed before the tap changed:

```bash
brew style Formula/git-slop.rb
brew fetch --force coreycoto/tap/git-slop
brew update
brew outdated --verbose coreycoto/tap/git-slop
brew upgrade coreycoto/tap/git-slop
test "$(git-slop version)" = "git-slop <version>"
brew test coreycoto/tap/git-slop
git-slop version
git slop --help
brew deps --installed coreycoto/tap/git-slop
```

Confirm the recorded package version changed from the Python-backed release to
the Rust release and that the installed formula no longer depends on Python,
libyaml, or Python package resources.

Second, use a separate clean macOS runner or host with no installed `git-slop`
formula:

```bash
test -z "$(brew list --versions git-slop)"
brew tap coreycoto/tap
brew install coreycoto/tap/git-slop
test "$(git-slop version)" = "git-slop <version>"
brew test coreycoto/tap/git-slop
git slop --help
```

The formula must keep the name `coreycoto/tap/git-slop` and must not depend on
Python, libyaml, or Python package resources.

## Verify Consumers

- Exercise the GitHub Action against a clean Linux consumer repository.
- Confirm the Action verifies `SHA256SUMS` before running the executable.
- Confirm existing Homebrew consumers upgrade in place.
- Confirm Windows and macOS archive installs can run `git-slop version`.
- Update consumer minimum-version pins only after GitHub Release, Homebrew, and
  any crates.io publication are independently verified.

## Close Out

- Confirm the GitHub Release notes summarize user-facing changes.
- Confirm the release tag, Cargo version, executable version, Homebrew version,
  and Action default all agree.
- Record any unsupported target or package-manager follow-up explicitly.
