# Release Publish

You are running in GitHub Actions inside the `git-slop` repository.

Use the custom agent `release_publisher` defined at
`.codex/agents/release-publisher.toml`. If that agent is unavailable, stop
immediately with an actionable error that names the missing agent file.
Use `$project-management-workflows:release-publish` as the canonical workflow
skill for this job.

## Read First

- `AGENTS.md`
- `.codex/README.md`

## Goal

Review and prepare release notes for the exact stable release candidate. Keep
the GitHub Release as a verified draft so a maintainer can publish the Action
to GitHub Marketplace through GitHub's required web approval.

## Boundaries

- Use checked-out repo files, Cargo and the private `xtask`, `gh`, GitHub
  tokens, and local CLI tooling only.
- This public release workflow must not acquire or invoke the private
  `agent-plugins` runtime and must not receive its read token.
- Do not assume Marketplace-installed connectors are available on the runner.
- Do not use the GitHub Git Data API.
- Treat the crates.io package digest, full source revision, exact semver tag,
  release manifest, native archives, Formula, and installed build identity as
  one immutable provenance chain.
- Accept a candidate only when `release-publish.yml` was dispatched on the
  exact current `main` revision and its Linux x86-64/ARM64, macOS Apple Silicon,
  Windows x86-64/ARM64 preflight lanes, and native Homebrew Formula audit all
  passed before the protected `release` environment was approved.
- The only exception to exact-revision `main` equality is explicit protected
  recovery after crates.io has already accepted the package. Recovery must be
  keyed by a full source revision and crate SHA-256. Keep the current workflow
  control revision separate: it must remain exact live `main` after protected
  approval and at tag mutation, while only the immutable release revision may
  be an older ancestor of `origin/main`. Reverify the non-yanked API/index and
  static package bytes plus embedded VCS revision, and do not invoke the OIDC
  authentication action or Cargo publication step. Recovery must not request
  or consume a crates.io credential.
- The crates.io Trusted Publisher identity is exactly repository
  `coreycoto/git-slop`, workflow filename `release-publish.yml`, and environment
  `release`. For crates.io authentication, only the protected `publish-crate`
  job may use an `id-token: write` grant to invoke the reviewed SHA-pinned
  `rust-lang/crates-io-auth-action`; the draft job's separate OIDC grant remains
  confined to build-provenance attestations.
- Exchange OIDC identity only in normal `publish` mode when the version is
  absent. Pass `steps.crates-io-auth.outputs.token` as
  `CARGO_REGISTRY_TOKEN` only to the immediately following exact Cargo
  publication step. Do not reference a long-lived crates.io secret, add a
  silent token fallback, print or persist the temporary token, or pass it to
  notes, build, archive, tag, release, Action, recovery, or Homebrew operations.
- Require the candidate `.crate`, crates.io index checksum, and downloaded
  static `.crate` to have one exact SHA-256. Crates.io publication precedes tag
  creation; the final archives and Formula derive from those registry bytes.
- Never move or delete an existing tag.
- Never publish a draft release, select Marketplace categories, or dispatch a
  Homebrew update from this Codex job. The protected workflow code owns its
  narrowly scoped immutable-identity dispatch; Codex does not.
- Marketplace publication is a manual GitHub UI gate with **Code quality** as
  the primary category and **Continuous integration** as the secondary. Do not
  publish the visible draft until the five-target Action smoke matrix and the
  terminal `marketplace-ready` job are green.

## Workflow

1. Read the version, tag, revision, and verified distribution evidence supplied
   by the release workflow.
2. Confirm the release notes match the checked-in scope and do not overstate
   unverified targets or package-manager availability.
3. Create or update notes only while the release is a draft. Refuse to mutate a
   published release or a release whose tag or assets disagree.
4. Leave the release as a draft and report the release URL plus the remaining
   Marketplace UI approval.
5. Explain that the already-approved protected publication job dispatches only
   the immutable version, revision, canonical crate URL, and crate digest to the
   Homebrew receiver. The receiver waits for the exact public stable release
   before deriving and reverifying Formula/manifest digests. The
   `release.published` workflow is read-only verification and requires no
   second environment approval. After the receiver's exact-head bottle tests
   pass, trusted tap `main` workflow code reverifies the current-parent,
   exact-head, two-file, artifact, and release contracts and publishes the tap
   change automatically; no label or tap environment approval is part of the
   normal path. `homebrew-handoff.yml` is protected manual recovery only.
6. Report rerun state explicitly. A matching package/tag/draft is resumable; a
   digest or revision mismatch fails closed; a published release is
   verification-only. `artifact_paths` may be empty when this job does not
   upload files.
7. In recovery mode, state the supplied version, full revision, crate SHA-256,
   containment proof, and whether the missing exact tag was created or already
   matched. Never describe recovery as rebuilding from current `main`.

Your final response must satisfy the structured output schema for this workflow.
