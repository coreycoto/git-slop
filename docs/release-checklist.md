# Git Slop Release Checklist

Use this checklist for stable releases of the Rust CLI, crates.io package,
GitHub Release archives, public GitHub Marketplace Action, Homebrew Formula,
and external Scoop manifest.
The canonical release identity is one strict `X.Y.Z` version and one full Git
commit. The crates.io package, `vX.Y.Z` tag, five native archives, release
manifest, installed binary, Action outputs, and Homebrew Formula must all agree
on that identity.

## One-Time Publisher Setup

- Enable two-factor authentication on the GitHub account that publishes the
  Marketplace listing and accept the GitHub Marketplace Developer Agreement.
- Keep a protected GitHub environment named `release`, restricted to the
  `main` branch, with exactly one required approval on the normal release path.
- In the `git-slop` crate settings on crates.io, add one GitHub
  [Trusted Publisher](https://crates.io/docs/trusted-publishing) with this exact
  identity:

  - repository owner: `coreycoto`;
  - repository name: `git-slop`;
  - workflow filename: `release-publish.yml`; and
  - environment: `release`.

- Leave **Require trusted publishing for all new versions** disabled until the
  first complete OIDC-backed patch release has been proven. Both credential
  methods remain accepted by crates.io during this migration window.
- Keep the existing crate-scoped API token and the GitHub `release` environment
  secret `CARGO_REGISTRY_TOKEN` available only as an inert rollback resource
  during that proof. `release-publish.yml` must not reference the secret or
  silently fall back to it; restoring token publication would require a
  separately reviewed exact-`main` workflow change.
- Store a fine-grained GitHub token that can dispatch the receiver workflow in
  `coreycoto/homebrew-tap` as the environment secret
  `HOMEBREW_TAP_DISPATCH_TOKEN`.
- Keep the tap receiver at `.github/workflows/update-git-slop.yml` on that
  repository's `main` branch.
- Keep the trusted-main publisher at `.github/workflows/publish.yml` on the tap
  repository's `main` branch. The final successful job in the canonical
  `Release git-slop bottles` workflow sends a `repository_dispatch` containing
  only its exact run ID; the publisher treats that payload as a pointer and
  revalidates the run and automation head through the Actions API.
- Store a separate fine-grained GitHub token as the `git-slop` repository
  secret `SCOOP_BUCKET_DISPATCH_TOKEN`. Give it access only to
  `coreycoto/scoop-bucket` with **Actions: read and write**; do not grant source,
  administration, pull-request, or contents write permission through that
  cross-repository token.
- Keep `.github/workflows/update-git-slop.yml` on exact trusted bucket `main`.
  The bucket repository's own workflow token performs the manifest branch, PR,
  native qualification, governed merge, and exact-main proof. Keep its Actions
  setting that permits workflows to create pull requests enabled.

The normal `github.token` creates the exact tag and GitHub Release in this
repository. No additional GitHub PAT is needed for those same-repository
operations. The Homebrew token is used only by the deliberate cross-repository
dispatch step inside the already-approved protected publication job. The Scoop
token is used only after the stable release is public, by one exact step in the
read-only publication-verification workflow; it introduces no second
environment approval. Neither token should be reused for the other package
manager. The existing `HOMEBREW_TAP_DISPATCH_TOKEN` does not need to be replaced
for this release unless it was exposed or its repository/permission scope is
wrong.

## Prepare Main

- Update `Cargo.toml`, `Cargo.lock`, the Action's default `version`, examples,
  and generated release-note inputs to the same stable version.
- Confirm there is no prerelease suffix or leading zero.
- Confirm `action.yml` remains the only root Action metadata file and its
  Marketplace name, description, branding, inputs, and outputs are current.
- Confirm the nested Actions in `action.yml` and release workflows are pinned
  to full commit SHAs.
- For crates.io authentication, confirm only the protected `publish-crate` job
  uses the minimum OIDC permission, `id-token: write`, and invokes
  `rust-lang/crates-io-auth-action@c6f97d42243bad5fab37ca0427f495c86d5b1a18`
  (`v1.0.5`). The action must have no custom registry input or fail-open
  behavior.
- Confirm `.github/workflows/release-publish.yml` contains no
  `secrets.CARGO_REGISTRY_TOKEN` reference. Its temporary
  `CARGO_REGISTRY_TOKEN` environment value must come only from
  `steps.crates-io-auth.outputs.token` on the exact Cargo publication step.
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
publication. In normal `publish` mode, and only when the version is still absent,
the job exchanges its GitHub OIDC identity through the reviewed crates.io auth
action. The resulting 30-minute token is passed as `CARGO_REGISTRY_TOKEN` only
to the immediately following `cargo publish --no-verify` step; the action's
post-step revokes it when the job completes. The standing GitHub environment
secret is not read.

The workflow packages the candidate again, requires byte-for-byte equality
with the preflight package, and then reconciles the registry even when Cargo
returns a timeout or another nonzero status. Publication is accepted only when
all of these values equal the candidate SHA-256:

- the crates.io index/API checksum;
- the downloaded static `.crate` checksum; and
- the locally verified candidate checksum.

A yanked version is rejected. Only after this verification does the workflow
create the immutable lightweight `v<version>` tag at the exact source revision.
An existing version/tag is a valid rerun only when version, revision, and crate
digest all agree; the workflow never moves or deletes a tag.

If the version already exists, the OIDC exchange and Cargo publication steps
are both skipped. Recovery mode is likewise unable to request or consume a
crates.io credential. These rerun paths reverify the immutable registry bytes
before any missing tag or release work.

After registry and tag verification, that same protected job sends only the
immutable version, source revision, canonical crates.io URL, and crate SHA-256
to the Homebrew tap receiver. The token is scoped to that one dispatch step.
The receiver waits for the exact public GitHub Release; it does not create a
tap PR from the unpublished draft or trust precomputed Formula/manifest
digests. This is the normal release path's only Actions environment approval.

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
  OIDC authentication action and Cargo publication step are unreachable in
  recovery mode, so recovery cannot request or consume a crates.io credential.
  The historical release revision remains the source of every artifact and of
  the composite Action that Marketplace consumers receive. Draft discovery,
  asset repair, and an initial installer verification may use current trusted
  control tooling, but terminal Marketplace readiness requires the exact
  historical tag to pass the full five-platform composite-Action smoke. If that
  tagged Action cannot pass, recovery stops instead of masking it with newer
  control code.
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
- A failed Homebrew receiver can be recovered by manually dispatching
  `homebrew-handoff.yml` from current `main` with the published version and
  source revision. That protected recovery workflow reverifies the public
  identity before redispatch; it cannot change the package, tag, or release
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
using the Marketplace controls. The Homebrew receiver may already be running;
its bounded public-release wait is expected and cannot bypass this draft gate.

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

The protected crates.io publication approval also authorizes one narrowly
scoped Homebrew receiver dispatch. The receiver starts with the immutable
version, revision, canonical crate URL, and crate SHA-256, then waits for the
exact stable GitHub Release to become public. Once the Marketplace publication
step makes that release public, the receiver downloads and verifies all release
assets, exact tag revision, schema-3 manifest, GitHub asset digests, static
crates.io package, Formula, and seven-line checksum inventory. It derives the
Formula and manifest URLs/digests from those verified public assets before
creating the tap PR.

The `release.published` event runs
`.github/workflows/release-published.yml`. Its first job remains read-only and
verifies the immutable public release; a dependency-ordered job then exposes
`SCOOP_BUCKET_DISPATCH_TOKEN` to exactly one `gh workflow run` command and sends
only the verified version, release ID, revision, and release-manifest digest to
the Scoop receiver. It never redispatches Homebrew and introduces no second
protected environment approval. If the early Homebrew receiver fails or times
out, explicitly dispatch `homebrew-handoff.yml` from current `main` with the
exact published version and source revision. That is a recovery path, not part
of a normal release.

The receiver opens an exact two-file automation PR and dispatches the canonical
two-platform `Release git-slop bottles` workflow. After every required
validation, bottle, and upgrade job succeeds, its final job sends a
`repository_dispatch` containing the exact successful run ID. The publisher
runs from trusted tap `main`, derives the run attempt, head SHA, branch, actor,
and conclusion from the Actions API, and then independently rechecks artifact
provenance, the unique same-repository bot PR, current `main` parent, exact head
SHA, two-file allowlist, release identity, Formula, manifest, and both unexpired
bottle artifacts. It repeats the parent/head/PR/two-file checks immediately
before `brew pr-pull`, publishes with the expected head SHA, and removes only
the consumed automation branch. A matching formula already on `main` is an
idempotent success only after the same canonical bottle block is verified. No
label or additional manual Actions approval is part of the normal path.

For bounded recovery after a publisher-only failure, the tap owner may resend
`git-slop-bottles-ready` with the same exact successful run ID while both
artifacts remain unexpired; the publisher revalidates the run and all current
state rather than trusting additional payload fields.

The resulting Formula must retain `coreycoto/tap/git-slop`, build from the exact
`.crate` source, and introduce no auxiliary runtime dependency. Homebrew derives
the version from the crates.io URL, so the Formula must not declare a redundant
`version` stanza; its embedded-provenance assertions must also pass Homebrew's
strict Ruby style.

The v0.9.3 closeout proved a crates.io-backed source Formula only. The Actions
incident and manual tap merge bypassed the exact-PR, two-bottle trusted-main
publication path, so v0.9.3 is not bottle-publication evidence. Record bottle
proof for a later release only when both exact artifacts and the automatic
trusted-main publication complete through the canonical path.

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

## Publish And Verify The External Scoop Manifest

Scoop publication follows the stable public GitHub Release. It is not a ninth
release asset, an eighth checksum entry, another protected-environment
approval, or a bucket credential shared with the source repository.

The automatic trusted-main Scoop receiver starts only after the read-only
public-release verifier has bound the exact version, numeric release ID, source
revision, and release-manifest SHA-256. Trusted bucket `main` independently
downloads the public release, requires the exact eight assets and seven
checksum entries, verifies every GitHub asset digest, resolves the tag, and
rerenders `bucket/git-slop.json`. The manifest must select
`git-slop-v<version>-x86_64-pc-windows-msvc.zip` for `64bit` and
`git-slop-v<version>-aarch64-pc-windows-msvc.zip` for `arm64`; each literal
hash must match both `SHA256SUMS` and the corresponding
`release-manifest.json` entry.

The receiver creates or reuses an exact one-file automation branch and
manifest-only pull request. It explicitly dispatches CI for that exact head,
requires the `Windows 64bit` and `Windows arm64` jobs to pass schema,
release-identity, hash-failure, and clean install/uninstall tests, then rechecks
the current base, bot PR, single-file allowlist, head, run, and job identities
immediately before merging through the active ruleset. Because merges made by a
workflow token do not recursively start ordinary push workflows, it explicitly
dispatches and awaits the same qualification on the resulting exact bucket
`main`. No per-release approval or manual merge is part of the normal path.

The installed binary must report the public tag's full source revision with
`source_dirty: false`, and both invocation forms must resolve:

```powershell
scoop bucket add coreycoto https://github.com/coreycoto/scoop-bucket
scoop install coreycoto/git-slop
git-slop version
git-slop build-info --format json
git slop version
scoop uninstall git-slop
```

After the exact bucket head merges, repeat a clean public-bucket install on each
architecture and record the source release SHA, current git-slop main SHA,
bucket main SHA, manifest URL, automation PR, exact-head and exact-main run IDs,
and two archive SHA-256 values.

Beginning with v0.9.6, every release after the first Scoop-published version
must also prove a cross-version upgrade-in-place on both Windows architectures.
For v0.9.6, begin with a verified public v0.9.5 installation, record its version
and full source revision, refresh the bucket, and update the existing package:

```powershell
git-slop version
git-slop build-info --format json
scoop update
scoop update git-slop
git-slop version
git-slop build-info --format json
git slop version
```

Record the pre-update and post-update versions, source revisions, architecture,
bucket main SHA, and manifest URL. If receiver recovery is necessary, manually
dispatch `Update git-slop manifest` on exact bucket `main` with the same four
immutable values; it is idempotent and accepts no caller-supplied archive URL
or Windows hash.

## Verify Consumers And Close Out

- Run the public Action on a clean Linux consumer and on the supported runner
  matrix when release risk warrants it.
- Confirm the Action outputs `source-revision`, `crate-sha256`, and
  `release-manifest-sha256` with the expected values.
- Confirm `cargo install git-slop --version <version> --locked` succeeds.
- Confirm the GitHub Release, Marketplace listing, crates.io version, Homebrew
  Formula, executable version, and full source revision all agree.
- For the Issue #69 migration proof, reserve v0.9.4 for this OIDC-backed patch
  release. Record the successful Release Publish run ID and exact source
  revision, then require the crates.io version API to attribute publication to
  that same GitHub repository, run, and revision:

  ```bash
  release_version=0.9.4
  release_revision=<40-character-release-revision>
  release_run_id=<release-publish-run-id>
  curl --fail --silent --show-error \
    --user-agent "git-slop-release-checklist/1 (https://github.com/coreycoto/git-slop)" \
    "https://crates.io/api/v1/crates/git-slop/${release_version}" \
    | jq -e \
      --arg version "$release_version" \
      --arg revision "$release_revision" \
      --arg run_id "$release_run_id" \
      '.version.num == $version
       and .version.trustpub_data.provider == "github"
       and .version.trustpub_data.repository == "coreycoto/git-slop"
       and .version.trustpub_data.sha == $revision
       and .version.trustpub_data.run_id == $run_id'
  ```

- Do not remove the rollback token merely because crates.io accepted v0.9.4.
  First complete the exact tag, package checksum, draft and published GitHub
  Release, Marketplace Action smoke, Homebrew Formula, consumer install, and
  post-publication verification gates above.
- Only after that proof is terminal, enable **Require trusted publishing for all
  new versions** in the crates.io `git-slop` settings and verify the public API:

  ```bash
  curl --fail --silent --show-error \
    --user-agent "git-slop-release-checklist/1 (https://github.com/coreycoto/git-slop)" \
    https://crates.io/api/v1/crates/git-slop \
    | jq -e '.crate.trustpub_only == true'
  ```

- Revoke the old crates.io API token in crates.io account settings, delete the
  inert GitHub environment secret, and verify it is absent:

  ```bash
  gh secret delete CARGO_REGISTRY_TOKEN \
    --repo coreycoto/git-slop \
    --env release
  gh secret list --repo coreycoto/git-slop --env release
  ```

  Keep the existing Homebrew dispatch token on its normal rotation schedule;
  rotate it immediately only if its value or scope was exposed.
