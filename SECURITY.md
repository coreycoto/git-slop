# Security Policy

## Reporting A Vulnerability

Please do not report security vulnerabilities through public GitHub issues.

Use GitHub private vulnerability reporting when it is available for this
repository. If private reporting is unavailable, contact the maintainer
directly at security@coreycoto.com.

Include:

- affected `git-slop` version or commit
- operating system, architecture, and `git-slop version` output
- installation method: Homebrew, release archive, tagged Cargo source, or Action
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
release archive and checksum manifest, publishes derived Markdown to the job
summary, and can upload a bounded set of derived report files. Pull request
comments and enforcement are opt-in. A report about the Action unexpectedly
uploading source files, bypassing checksum verification, or exceeding those
configured boundaries is in scope.

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

Execution-state sync runs on `pull_request_target` and checks out only the
trusted base before acquiring the private runtime. Its project credential is
separate from runtime acquisition and is not job-scoped: only the direct
project snapshot and execution-state steps receive that resolved `GH_TOKEN`.
Preparation, offline verification, and publisher identity/interpreter smoke
therefore cannot inherit the project PAT.

For privileged `pull_request_target` automation, the workflow first checks out
and validates the trusted base, then snapshots its Codex config, profiles,
agents, prompt, and output schema under the ephemeral runner directory. Only
after runtime verification and embedded marketplace installation does it check
out the requested head, without persisted checkout credentials. No head-owned
maintainer code or Codex control file is executed; `github.token` is exposed
only to the deliberate Codex mutation step.
