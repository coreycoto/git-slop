# Security Policy

## Reporting A Vulnerability

Please do not report security vulnerabilities through public GitHub issues.

Use GitHub private vulnerability reporting when it is available for this
repository. If private reporting is unavailable, contact the maintainer
directly at security@coreycoto.com.

Include:

- affected `git-slop` version or commit
- operating system, architecture, and `git-slop version` output
- installation method: crates.io, Homebrew Formula, release archive, or Action
- exact command or artifact involved
- impact and reproduction steps
- any suggested fix, if you have one

## Scope

This policy covers the native Rust `git-slop` CLI, Cargo source package,
checksummed release archives, Homebrew formula, GitHub Action installer and
runner, checked-in plugin guidance, and the private standalone Rust `xtask`
validation and release tooling. Workflow acquisition of the manifest-pinned
`gh-steward` native extension and selected public Agent Plugins sources is in
scope at this repository's consumer boundary; each publisher maintains its
own implementation and release process.

Out of scope:

- findings that require access to a user's local repository or credentials
- third-party dependency vulnerabilities that should be reported upstream
- non-security correctness issues in detector scoring or report interpretation

The CLI is a local-first analysis tool. It must not upload repository contents,
invoke hosted models for scoring, mutate GitHub, or automatically modify code.
It shells out to the local Git executable to inventory tracked files and read
history.

The GitHub Action has an explicit hosted boundary: it downloads the selected
release archive, checksum inventory, and schema-3 release manifest. Before
execution it verifies the GitHub asset digests, exact release tag revision,
archive digest, canonical crates.io package digest, and the installed binary's
embedded `build-info` provenance. It publishes derived Markdown to the job
summary and can upload a bounded set of derived report files. Pull request
comments and enforcement are opt-in. A report about the Action unexpectedly
uploading source files, bypassing provenance verification, accepting an unsafe
archive, or exceeding those configured boundaries is in scope.

The crates.io check is an unauthenticated, bounded download from the canonical
static package URL; the GitHub token is never sent to crates.io. The Action
checks both the package SHA-256 and its embedded clean VCS revision. It resolves
the release through the exact tag namespace with bounded annotated-tag peeling,
never through a potentially ambiguous branch name. Native release archives are
limited to 128 MiB in both publisher validation and consumer installation.

The stable release workflow starts only through `workflow_dispatch` at exact
current `main`. All seven target builds and distribution metadata pass preflight
before the protected `release` environment can expose the one-time crates.io
bootstrap token. The candidate package, crates.io index checksum, and
downloaded static `.crate` must have one SHA-256 digest. Automation creates the
tag only after that package is verified, then builds the release archives from
those registry bytes and stops at a verified draft. Publishing the Action to
Marketplace remains a deliberate browser approval with 2FA. The published
release triggers a same-repository `github.token` relay with no named secret,
followed by a separately protected `main`-branch Homebrew handoff; only its
final dispatch step receives the existing `HOMEBREW_TAP_DISPATCH_TOKEN`.

Maintainer workflows acquire the native extension from an exact source
revision and target-specific digest, verify the publisher's attested release,
and stage it under ephemeral runner storage. Codex CLI and selected public
plugins are installed only under a temporary `CODEX_HOME`; no global extension
or plugin installation is changed. No private publisher runtime, read token,
Python SDK, or PEX is used.

Execution-state sync runs on `pull_request_target` using trusted base content.
Repository and Project credentials remain step-scoped to the reads and writes
that need them. Before a new plan, the workflow reads complete workflow and
artifact history. After dispatch, its run-scoped artifact retains the exact
target, run identity, reviewed plan, apply receipt, and native journal. A retry
may continue only from matching complete evidence; missing, expired,
truncated, mismatched, or ambiguous recovery state produces `recovery_needed`
and blocks a fresh plan. Target concurrency never cancels an in-flight run,
and uncertain writes are not replayed automatically.

For privileged `pull_request_target` automation, the workflow first checks out
and validates the trusted base, then snapshots its Codex config, profiles,
agents, prompt, and output schema under the ephemeral runner directory. Only
after source verification and isolated plugin installation does it check
out the requested head, without persisted checkout credentials. No head-owned
maintainer code or Codex control file is executed; `github.token` is exposed
only to the deliberate Codex mutation step.
