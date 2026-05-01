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

- Python 3.13
- `uv`
- Git

```bash
git clone https://github.com/coreycoto/git-slop.git
cd git-slop
uv sync --group dev
uv run git-slop version
```

Optional maintainer-agent tests use the `maintainer` dependency group:

```bash
uv sync --group dev --group maintainer
```

Those tests are skipped when `agent-plugins` is not available.

## Validation

Run these before submitting a pull request:

```bash
uv run ruff check
uv run pytest
```

For packaging or release-script changes, also run:

```bash
uv build
```

For plugin or maintainer workflow changes, also run:

```bash
uv run python scripts/validate_codex_surface.py
uv run python scripts/smoke_plugin_consumer.py
```

## Project Boundaries

Keep `git-slop` local-first and deterministic:

- no hosted API dependency for detector scoring
- no LLM-backed scoring
- no hidden overlay score inflation
- no automatic code mutation, commits, or pushes
- no GitHub mutation from the public CLI
- no broad report schema changes without tests and docs

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
