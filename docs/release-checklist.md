# Git Slop Release Checklist

Use this checklist for stable releases of the Rust CLI, crates.io package,
GitHub Release archives, public GitHub Marketplace Action, and Homebrew Formula.
The canonical release identity is one strict `X.Y.Z` version and one full Git
commit. The crates.io package, `vX.Y.Z` tag, five native archives, release
manifest, installed binary, Action outputs, and Homebrew Formula must all agree
on that identity.

## One-Time Publisher Setup

- Enable two-factor authentication on the GitHub account that publishes the
  Marketplace listing and accept the GitHub Marketplace Developer Agreement.
- Keep a protected GitHub environment named `release`, restricted to the
  `main` branch. A required reviewer is recommended.
- For the first publication only, store a short-lived crates.io token scoped to
  `publish-new` as the environment secret `CARGO_REGISTRY_TOKEN`. Treat it as a
  namespace-bootstrap credential, not the permanent release mechanism.
- Until the Trusted Publishing migration in issue #69 is implemented and
  verified, subsequent releases use a crates.io API token scoped to
  `publish-update` for exactly the `git-slop` crate under the same environment
  secret name. Keep it available only to the deliberate publication step.
- Store a fine-grained GitHub token that can dispatch the receiver workflow in
  `coreycoto/homebrew-tap` as the environment secret
  `HOMEBREW_TAP_DISPATCH_TOKEN`.
- Keep the tap receiver at `.github/workflows/update-git-slop.yml` on that
  repository's `main` branch.

The normal `github.token` creates the exact tag and GitHub Release in this
repository. No additional GitHub PAT is needed for those same-repository
operations. The Homebrew token is used only by the final cross-repository
dispatch step. The existing `HOMEBREW_TAP_DISPATCH_TOKEN` does not need to be
replaced for this release unless it was exposed or its repository/permission
scope is wrong.

## Prepare Main

- Update `Cargo.toml`, `Cargo.lock`, the Action's default `version`, examples,
  and generated release-note inputs to the same stable version.
- Confirm there is no prerelease suffix or leading zero.
- Confirm `action.yml` remains the only root Action metadata file and its
  Marketplace name, description, branding, inputs, and outputs are current.
- Confirm the nested Actions in `action.yml` and release workflows are pinned
  to full commit SHAs.
- Run the complete local validation from a clean worktree:

```bash
cargo xtask release-prepare --version <version> --check-only
cargo fmt -p git-slop -- --check
cargo clippy -p git-slop --all-targets --all-features --locked -- -D warnings
cargo test -p git-slop --all-targets --all-features --locked
cargo fmt --manifest-path xtask/Cargo.toml --all -- --check
cargo clippy --manifest-path xtask/Cargo.toml --all-targets --all-features --locked -- -D warnings
cargo test --manifest-path xtask/Cargo.toml --all-targets --all-features --locked
cargo xtask validate
node --test action/*.test.mjs
cargo publish -p git-slop --dry-run --locked
```

Do not create or push the release tag manually. The workflow deliberately
creates it only after crates.io has accepted and served the exact candidate
package.

## Start The Release

Dispatch `.github/workflows/release-publish.yml` from the exact current `main`
revision with the stable version:

```bash
gh workflow run release-publish.yml \
  --repo coreycoto/git-slop \
  --ref main \
  --field mode=publish \
  --field version=<version>
```

Before the protected environment is entered, the workflow:

1. revalidates that the dispatch revision is the live `main` revision;
2. runs the full product, `xtask`, Action, package, and publish dry-run gates;
3. creates and verifies the exact candidate `.crate` bytes;
4. builds and smokes all five supported targets from those candidate bytes;
5. checks `git-slop version` and `git-slop build-info --format json`;
6. dry-runs schema-3 manifest and crates-backed Formula generation; and
7. audits and styles the generated Formula with native Homebrew on macOS.

The five targets are Linux x86-64, Linux ARM64, macOS Apple Silicon, Windows
x86-64, and Windows ARM64. macOS Intel is not a release target.

## Approve crates.io Publication

Review the preflight jobs, then approve the `release` environment. After
approval the protected job re-fetches live `main`; any drift from the candidate
revision fails closed in normal `publish` mode. In both modes, the separate
workflow control revision must still equal live `main` after approval and again
at the tag mutation boundary. If `main` advances while the run is waiting,
dispatch the workflow again from the new head. Recovery permits only the
immutable release revision—not the workflow control revision—to be an older
ancestor of `main`.

The protected publication job cannot start until the native Homebrew audit has
accepted the exact candidate Formula. The first public mutation is crates.io
publication. The workflow packages the candidate again, requires byte-for-byte
equality with the preflight package, and runs `cargo publish --no-verify`. It
then reconciles the registry even when Cargo returns a timeout or another
nonzero status. Publication is accepted only when all of these values equal the
candidate SHA-256:

- the crates.io index/API checksum;
- the downloaded static `.crate` checksum; and
- the locally verified candidate checksum.

A yanked version is rejected. Only after this verification does the workflow
create the immutable lightweight `v<version>` tag at the exact source revision.
An existing version/tag is a valid rerun only when version, revision, and crate
digest all agree; the workflow never moves or deletes a tag.

## Reruns And Failures

The workflow is deliberately restartable without weakening immutable identity:

- Before crates.io publication, a failure has made no public release mutation;
  fix the candidate on `main` and dispatch the resulting exact revision.
- If Cargo reports an error after accepting the package and the release revision
  is still live `main`, rerun in `publish` mode. The workflow reconciles
  crates.io and proceeds only when the local, index, and static-package digests
  are identical.
- If crates.io accepted the package but `main` advanced before the exact tag or
  draft was completed, use the explicit protected recovery mode. Supply the
  original full source revision and crate SHA-256; do not substitute the new
  `main` revision or a newly packaged digest. Copy both values from the failed
  run's **Immutable release identity** job summary and cross-check the SHA-256
  against the crates.io API before dispatching:

  ```bash
  gh workflow run release-publish.yml \
    --repo coreycoto/git-slop \
    --ref main \
    --field mode=recover \
    --field version=<version> \
    --field recovery_revision=<40-character-release-revision> \
    --field recovery_crate_sha256=<64-character-crates.io-sha256>
  ```

  Recovery runs the workflow definition from exact current `main`, carries that
  workflow control revision separately from the historical release revision,
  and requires the control revision to remain live `main` after protected
  approval and at any missing-tag push. The supplied release revision must
  remain an ancestor of current `origin/main`. The non-yanked crates.io API
  checksum, downloaded static `.crate`, embedded Cargo VCS revision, and
  supplied digest must agree. Recovery reacquires the immutable crate instead
  of repackaging advanced `main`, re-runs all five target lanes, and enters the
  same protected `release` environment before any missing tag is pushed. The
  Cargo publication step and its secret are unreachable in recovery mode. The
  historical release revision remains the source of every artifact and of the
  composite Action that Marketplace consumers receive. Draft discovery, asset
  repair, and an initial installer verification may use current trusted control
  tooling, but terminal Marketplace readiness requires the exact historical tag
  to pass the full five-platform composite-Action smoke. If that tagged Action
  cannot pass, recovery stops instead of masking it with newer control code.
- A missing tag is created only after the registry package has been reverified.
  An existing tag must already resolve to the supplied revision; the workflow
  never moves or deletes it. A missing/yanked package, a revision no longer
  contained in `main`, or any revision/digest mismatch fails closed and requires
  investigation rather than mutation.
- A draft release may be refreshed only with the same verified identity. Once
  published, release assets are treated as immutable and the release job is a
  verification-only no-op. Draft metadata is resolved to a numeric GitHub
  Release ID before upload and verification because the tag-indexed REST
  endpoint does not expose drafts.
- A failed `release.published` relay or Homebrew handoff can be rerun from the
  published identity; neither is allowed to change the package, tag, or release
  assets.

## Review The Verified Draft

The workflow builds the five final archives from the downloaded crates.io
package, verifies their embedded build identity, and creates or refreshes a
draft GitHub Release. It never publishes the release automatically.

The draft must contain exactly eight assets:

- five target archives;
- `SHA256SUMS`, with exactly seven unique entries;
- `release-manifest.json`, schema 3; and
- `git-slop.rb`, whose source URL and SHA-256 point to the static crates.io
  package rather than a GitHub archive or Homebrew bottle.

`SHA256SUMS` covers the five archives, manifest, and Formula. GitHub's release
asset digests, the manifest's target matrix and source provenance, the exact
tag commit, the crate checksum, and the Action installer must all verify before
the draft is ready.

Inspect the draft and workflow summary:

```bash
gh run list --repo coreycoto/git-slop --workflow release-publish.yml --limit 1
gh release view v<version> --repo coreycoto/git-slop --json url,tagName,isDraft,isPrerelease,assets
```

Do not edit or publish the draft merely because it is visible. Draft creation
precedes the five-platform Action smoke matrix. Wait until the complete Release
Publish run is green, including the terminal `marketplace-ready` job, before
using the Marketplace controls.

## Publish The Action In GitHub Marketplace

Open the verified draft release in GitHub's web interface:

1. choose **Edit**;
2. select **Publish this Action to the GitHub Marketplace**;
3. use **Code quality** as the primary category and **Continuous integration**
   as the secondary category;
4. review the Marketplace terms and complete the 2FA prompt; and
5. publish the release.

This UI approval is intentional: GitHub does not expose a supported workflow
or REST API switch for a new Action listing's Marketplace checkbox and
categories. Publishing the release makes `coreycoto/git-slop@v<version>`
available; the Action still installs the verified prebuilt archive, never
Homebrew and never an unverified executable.

## Verify The Homebrew Handoff

The `release.published` event runs `.github/workflows/release-published.yml`.
That relay uses only its same-repository `github.token`, receives no named
secret, verifies the public release, and dispatches
`.github/workflows/homebrew-handoff.yml` from `main`. GitHub does not expose the
Marketplace checkbox or categories to this relay. Before approving the
protected `release` environment, open the public Marketplace listing and verify
that it visibly shows the exact `v<version>` tag/version. If it does not, deny
or leave the deployment pending, repair the Marketplace publication in the
release UI, and rerun the handoff if needed. Never approve Homebrew solely
because the `release.published` event fired.

The handoff downloads and verifies all release assets, the exact tag revision,
schema-3 manifest, GitHub asset digests, static crates.io package, Formula, and
seven-line checksum inventory. Only its final step receives
`HOMEBREW_TAP_DISPATCH_TOKEN`; it sends the exact version, revision, URLs, and
digests to `coreycoto/homebrew-tap`.

Verify the tap workflow and Formula before merging its change. The Formula must
retain `coreycoto/tap/git-slop`, build from the exact `.crate` source, and
introduce no auxiliary runtime dependency. Homebrew derives the version from
the crates.io URL, so the Formula must not declare a redundant `version` stanza;
its embedded-provenance assertions must also pass Homebrew's strict Ruby style.

Test both upgrade and clean-install lanes:

```bash
brew update
brew upgrade coreycoto/tap/git-slop
test "$(git-slop version)" = "git-slop <version>"
git-slop build-info --format json
brew test coreycoto/tap/git-slop
```

On a clean host, replace `brew upgrade` with `brew install
coreycoto/tap/git-slop`.

## Verify Consumers And Close Out

- Run the public Action on a clean Linux consumer and on the supported runner
  matrix when release risk warrants it.
- Confirm the Action outputs `source-revision`, `crate-sha256`, and
  `release-manifest-sha256` with the expected values.
- Confirm `cargo install git-slop --version <version> --locked` succeeds.
- Confirm the GitHub Release, Marketplace listing, crates.io version, Homebrew
  Formula, executable version, and full source revision all agree.
- [Issue #69](https://github.com/coreycoto/git-slop/issues/69) tracks
  configuring a crates.io Trusted Publisher for
  `coreycoto/git-slop`, the exact release workflow, and the protected `release`
  environment. That migration updates the publish job to the reviewed OIDC
  contract and proves one token-free release before removing the API-token
  path; an open migration issue does not weaken or bypass the current protected
  token-backed release contract.
- Revoke the crates.io API token and remove `CARGO_REGISTRY_TOKEN` only after
  Trusted Publishing is configured and verified.
  Keep the existing Homebrew dispatch token on its normal rotation schedule;
  rotate it immediately only if its value or scope was exposed.
