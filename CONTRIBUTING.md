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
cargo build --locked
cargo run --locked -- version
```

The public runtime lives in the Rust modules under `src/` (including focused
submodules under `src/health/`, `src/overlays/`, `src/report/`, and
`src/report_ops/`). The retained `git-slop-maintainer` Python project under
`src/git_slop/` validates repo-local Codex, plugin, workflow, and release
wiring. It has no `git-slop` console entry point, contains no analyzer or
workflow implementation, and is not included in the Cargo package or native
release archives.

Python-facing validation requires Python 3.13 and `uv`:

```bash
uv sync --group dev
uv run ruff check
uv run pytest
```

Reusable `agent_plugins` behavior tests, marketplace bootstrap tests, and
clean-room plugin consumer smoke run in the `coreycoto/agent-plugins`
publisher repository. They are intentionally not duplicated here.

## Validation

Run these before submitting a pull request:

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features --locked -- -D warnings
cargo test --all-targets --all-features --locked
```

If the change touches Python maintainer tooling, maintainer scripts, plugin
validation, or workflow-contract tests, also run:

```bash
uv run ruff check
uv run pytest
```

For local plugin or maintainer workflow wiring changes, also run:

```bash
uv run python scripts/validate_codex_surface.py
```

To exercise a pinned workflow runtime itself, sync the publisher dependency
with `uv sync --group dev --group maintainer` and use the
`agent_plugins.marketplace.bootstrap` module named in the workflow.

For packaging or release-script changes, also run:

```bash
cargo package --locked
cargo publish --dry-run --locked
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
- no product detector, report, explain, plan, or CLI behavior in Python

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
