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
runner, and checked-in plugin guidance. It also covers retained Python
compatibility and maintainer tooling when that code participates in repository
validation or release automation.

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
