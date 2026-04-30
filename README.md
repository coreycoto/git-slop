# Git Slop

Find the files that cost too much context.

Git Slop is a deterministic, local-first detector for AI-era repositories. It
answers one stable question:

> Which files cost too much context to load, reason about, and safely change?

It also emits a separate overlay evidence layer for duplicated, scattered,
weakly verified, hard-to-navigate, or high-coordination concepts. Hotspot
scores and overlay evidence stay separate on purpose.

## Core Contract

Git Slop's stable detector surfaces are:

- `priority_score`
- `priority_band`
- `context_band`
- `git slop check`

Those surfaces do not silently absorb organization, verification, navigation,
blast-radius, stewardship, or semantic-drift overlays. Overlays are supporting
evidence, not correctness proofs and not score inflation.

## Install

### Homebrew

```bash
brew tap coreycoto/tap
brew install coreycoto/tap/git-slop
git-slop version
```

### GitHub Release Wheel

Tagged releases publish a wheel, source distribution, and
`release-manifest.json`.

```bash
gh release download v0.8.1 --repo coreycoto/git-slop \
  --pattern 'git_slop-*.whl' \
  --pattern release-manifest.json \
  --dir .artifacts/git-slop
shasum -a 256 .artifacts/git-slop/git_slop-0.8.1-py3-none-any.whl
uv tool install --force .artifacts/git-slop/git_slop-0.8.1-py3-none-any.whl
git-slop version
```

### From Source

```bash
git clone https://github.com/coreycoto/git-slop.git
cd git-slop
uv sync --group dev
uv run git-slop version
```

## Quickstart

```bash
git-slop init
git-slop find
git-slop show README.md
git-slop explain --top 5
git-slop plan --path src/git_slop --format json > .slop/latest/plan.json
git-slop refactor-preview --plan .slop/latest/plan.json
git-slop sarif --report .slop/latest/report.json --output .slop/latest/git-slop.sarif
git-slop compare --base .slop/runs/<old>/report.json --head .slop/latest/report.json
git-slop check
```

The package exposes both `git-slop ...` and `python -m git_slop ...`. When the
executable is on `PATH`, Git also accepts `git slop ...`.

## Command Surface

- `git slop init`: create `.slop/config.yaml` and generated-state ignores
- `git slop find`: analyze the current repo and write `.slop/latest/`
- `git slop show`: inspect one file from an existing report
- `git slop explain`: explain a file, folder, cluster, relationship, or top-N
- `git slop plan`: propose bounded maintenance slices from existing evidence
- `git slop compare`: compare two existing schema-3 reports
- `git slop sarif`: export action-queue findings as SARIF 2.1.0
- `git slop refactor-preview`: turn a plan payload into read-only next steps
- `git slop check`: run the stable detector gate
- `git slop version`: print the installed version

See [Command Guide](docs/commands.md) for options, examples, prompt packs, SARIF,
compare, and refactor-preview details.

## Generated State

Git Slop writes generated state under `.slop/`.

Commit:

- `.slop/config.yaml` when the repository intentionally configures Git Slop
- `.slop/.gitignore` so generated report/cache paths stay untracked

Do not commit routine generated outputs:

- `.slop/latest/`
- `.slop/runs/`
- `.slop/cache/`
- prompt packs
- SARIF exports
- plan, compare, or refactor-preview JSON

Upload generated outputs as CI artifacts when needed. Only check in derived
artifacts when they are intentionally curated examples or fixtures outside the
runtime `.slop/` state tree.

See [.slop Directory Policy](docs/slop-directory.md).

## What Git Slop Measures

Stable hotspot costs:

- load cost from context-token size and concentration
- volatility cost from age, churn, token churn, and recency-weighted activity
- coordination cost from change diffusion and co-change spread

Always-on overlays:

- organization health
- verification
- navigation
- blast radius
- stewardship
- semantic drift

Git Slop keeps context tokens and structural tokens separate. Context tokens
drive budget and load math; structural tokens drive duplication, cohesion,
navigation, and drift evidence.

## What Git Slop Does Not Do

- rewrite code automatically
- require hosted APIs
- send repo data anywhere
- use an LLM for scoring
- treat detector findings as correctness proofs
- fold overlays into `priority_score`

## Project Docs

- [Command Guide](docs/commands.md)
- [Report and Config Contract](docs/report-contract.md)
- [.slop Directory Policy](docs/slop-directory.md)
- [Architecture](docs/architecture.md)
- [Scoring Model](docs/scoring-model.md)
- [Roadmap](docs/roadmap.md)
- [Vision](docs/vision.md)
- [Release Checklist](docs/release-checklist.md)
- [Security Policy](SECURITY.md)
- [Contributing](CONTRIBUTING.md)

The Codex plugin is published from this repo as
`git-slop@git-slop-marketplace`. It owns product-specific install, report,
interpretation, planning, and consumer-adoption guidance. Reusable project and
backlog workflows live in the separate `coreycoto/agent-plugins` plugin.
