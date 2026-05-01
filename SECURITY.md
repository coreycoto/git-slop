# Security Policy

## Reporting A Vulnerability

Please do not report security vulnerabilities through public GitHub issues.

Use GitHub private vulnerability reporting when it is available for this
repository. If private reporting is unavailable, contact the maintainer
directly at security@coreycoto.com.

Include:

- affected `git-slop` version or commit
- operating system and Python version
- exact command or artifact involved
- impact and reproduction steps
- any suggested fix, if you have one

## Scope

This policy covers the `git-slop` CLI, Python package, release artifacts,
Homebrew formula, and checked-in plugin guidance.

Out of scope:

- findings that require access to a user's local repository or credentials
- third-party dependency vulnerabilities that should be reported upstream
- non-security correctness issues in detector scoring or report interpretation

`git-slop` is a local-first analysis tool. It should not upload repository
contents, invoke hosted models for scoring, mutate GitHub, or automatically
modify code.
