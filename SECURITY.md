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
validation and release tooling. Workflow use of the manifest-pinned external
`agent-plugins` prebuilt runtime is also in scope at this repository's
invocation boundary; its implementation is maintained in the publisher
repository.

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
current `main`. All five target builds and distribution metadata pass preflight
before the protected `release` environment can expose the one-time crates.io
bootstrap token. The candidate package, crates.io index checksum, and
downloaded static `.crate` must have one SHA-256 digest. Automation creates the
tag only after that package is verified, then builds the release archives from
those registry bytes and stops at a verified draft. Publishing the Action to
Marketplace remains a deliberate browser approval with 2FA. The published
release triggers a same-repository `github.token` relay with no named secret,
followed by a separately protected `main`-branch Homebrew handoff; only its
final dispatch step receives the existing `HOMEBREW_TAP_DISPATCH_TOKEN`.

Private maintainer workflows have a separate acquisition boundary. The
consumer manifest pins the publisher source revision and archive SHA-256; the
wrapper must also validate release metadata, the exact Linux target and archive
member, safe extraction, and the SCIE's embedded revision. The read token is
scoped only to preparation, the verified runtime lives under the job's
ephemeral runner directory without Actions caching, and subsequent marketplace
and governance commands run from embedded content without network acquisition.
Fork pull requests do not receive this secret, and pull-request-controlled code
must never perform preparation. The public release workflow never receives or
uses the private runtime token.

Execution-state sync runs on `pull_request_target` and checks out only trusted
base content before acquiring the private runtime. Active PR events pin the
base revision carried by the PR payload; a closed event uses the event's
current trusted base SHA so a merged PR cannot resurrect the pre-merge runtime
launcher. Its project credential is separate from runtime acquisition and is
not job-scoped: only the direct project snapshot and execution-state steps
receive that resolved `GH_TOKEN`. Preparation, offline verification, and
publisher identity/interpreter smoke therefore cannot inherit the project PAT.

For privileged `pull_request_target` automation, the workflow first checks out
and validates the trusted base, then snapshots its Codex config, profiles,
agents, prompt, and output schema under the ephemeral runner directory. Only
after runtime verification and embedded marketplace installation does it check
out the requested head, without persisted checkout credentials. No head-owned
maintainer code or Codex control file is executed; `github.token` is exposed
only to the deliberate Codex mutation step.
