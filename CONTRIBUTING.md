# Contributing

Thanks for taking the time to improve `git-slop`.

Please read [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md) before participating. For
security reports, follow [SECURITY.md](SECURITY.md) instead of opening a public
issue.

## Before You Start

Open an issue first for feature work, behavior changes, report schema changes,
or anything that could affect scoring. Small documentation fixes and clearly
scoped bug fixes can go straight to a pull request.

Good first contribution areas:

- documentation and examples
- fixture coverage for existing report surfaces
- small CLI bug fixes
- tests that pin deterministic behavior

## Local Setup

Use the local setup below for development and contributions. For normal CLI
usage, install Git Slop with Homebrew as described in
[docs/install.md](docs/install.md).

Requirements:

- Rust 1.85 or newer
- Cargo
- Git

```bash
git clone https://github.com/coreycoto/git-slop.git
cd git-slop
cargo build -p git-slop --locked
cargo run -p git-slop --locked -- version
```

The public runtime lives in the Rust modules under `src/` (including focused
submodules under `src/health/`, `src/overlays/`, `src/report/`, and
`src/report_ops/`). Repo-local Codex, plugin, workflow, repository, and release
contracts live in the private standalone Rust workspace under `xtask/`. The
root workspace excludes it, and the public `git-slop` package and native
release archives do not contain it. Its committed `xtask/Cargo.lock` keeps
maintainer validation reproducible independently of the public dependency graph.

Reusable `agent_plugins` behavior tests, marketplace bootstrap tests, and
clean-room plugin consumer smoke run in the `coreycoto/agent-plugins`
publisher repository. They are intentionally not duplicated here. The
`scripts/with-agent-plugins.sh` wrapper resolves a private Linux PEX SCIE from
the exact release, 40-character source revision, archive member, and SHA-256
digest in `.agents/plugins/marketplace-source.json`. Preparation uses an
ephemeral per-job directory; verification checks release metadata, archive
safety, digest, target, and embedded revision before direct CLI execution. It
does not create or sync a project dependency environment in this repository.

## Validation

Run these before submitting a pull request:

```bash
cargo fmt -p git-slop -- --check
cargo clippy -p git-slop --all-targets --all-features --locked -- -D warnings
cargo test -p git-slop --all-targets --all-features --locked
cargo fmt --manifest-path xtask/Cargo.toml --all -- --check
cargo clippy --manifest-path xtask/Cargo.toml --all-targets --all-features --locked -- -D warnings
cargo test --manifest-path xtask/Cargo.toml --all-targets --all-features --locked
cargo xtask validate
```

For focused maintainer-contract changes, the corresponding commands are:

```bash
cargo xtask validate-codex
cargo xtask validate-workflows
cargo xtask check-issue-forms
cargo xtask check-distribution
```

Use `cargo xtask validate-codex --require-codex-cli` when the local check must
also prove that the Codex CLI is installed. To exercise a pinned publisher
runtime itself, prepare and verify it through `scripts/with-agent-plugins.sh`,
then use its direct `marketplace` or `github` commands. The read token belongs
only on the prepare command. Do not add a parallel maintainer runtime or local
publisher dependency environment; interpreter mode is confined to isolated
runtime identity verification and the legacy compatibility entry point.

Keep the execution-state project PAT step-scoped to its two direct operations;
runtime preparation and verification must not inherit it. For privileged
`pull_request_target`, keep the repository token on the deliberate Codex
mutation step, validate and snapshot the trusted base Codex inputs before
checking out the requested head, and never run head-owned `xtask`, prompts,
schemas, config, or agents with that credential.

For packaging or release-script changes, also run:

```bash
cargo xtask check-distribution
cargo package -p git-slop --locked
cargo publish -p git-slop --dry-run --locked
```

The dry run validates package readiness; it does not mean a crates.io version
has been published.

## Project Boundaries

Keep `git-slop` local-first and deterministic:

- no hosted API dependency for detector scoring
- no LLM-backed scoring
- no hidden overlay score inflation
- no automatic code mutation, commits, or pushes
- no GitHub mutation from the public CLI
- no broad report schema changes without tests and docs
- no repository-owned maintainer runtime outside Rust; the pinned
  `agent-plugins` SCIE is acquired as a verified executable
- no product detector, report, explain, plan, or CLI behavior outside Rust

Validation and dogfood may use private or external repositories, but committed
repo content must not name them. Use neutral labels such as `local repo`,
`mature validation repo`, `smaller application repo`, or `consumer toolkit repo`.

## Pull Requests

In your PR description, include:

- summary of the change
- linked issue, when there is one
- validation commands run
- any report schema, CLI, or release-artifact impact

Maintainers may ask for a narrower scope when a PR mixes product behavior,
automation, and documentation changes.
