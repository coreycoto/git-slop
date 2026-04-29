# Git Slop

Find the files that cost too much context.

Git Slop is a deterministic, local-first detector for AI-era repositories. It
keeps one stable promise at the center of the product:

> Which files cost too much context to load, reason about, and safely change?

It now also emits a separate always-on overlay layer for structural and
operational evidence:

> Which concepts cost too much coordination because they are duplicated,
> scattered, weakly verified, hard to navigate, or forced to co-change?

Those are related questions, but Git Slop keeps them separate on purpose.

## Core Contract

The hotspot queue remains a context-cost detector:

- `priority_score`
- `priority_band`
- `context_band`
- `git slop check`

Those surfaces stay stable and explainable. They do not silently absorb
organization, verification, navigation, blast-radius, stewardship, or semantic
drift overlays.

The overlay layer is evidence-first:

- deterministic
- repo-local
- language-agnostic
- always emitted
- not a correctness oracle

## What Git Slop Measures

### Stable hotspot costs

- **Load cost**: context-token size and concentration
- **Volatility cost**: age, churn, token churn, and recency-weighted activity
- **Coordination cost**: change diffusion and co-change spread

### Always-on overlays

- **Organization health**: duplication, scatter, cohesion, and boundary leakage
- **Verification**: nearby-test and historical test-cochange evidence
- **Navigation**: ambiguity, path depth, sibling width, and term dispersion
- **Blast radius**: temporal coupling and average changeset spread
- **Stewardship**: author concentration and maintainer diversity
- **Semantic drift**: high-signal terms whose neighborhoods diverge across roots

## Context Tokens vs Structural Tokens

Git Slop intentionally uses two token pipelines:

- **Context tokens**
  - `tiktoken`-aligned
  - used for load and context-budget measurements
- **Structural tokens**
  - normalized lexical/path tokens
  - used for duplication, cohesion, drift, navigation, and boundary analysis

This separation keeps context-budget math honest without forcing the structural
layer to reuse the wrong representation.

## Quickstart

```bash
uv run git-slop init
uv run git-slop find
uv run git-slop show README.md
uv run git-slop explain --top 5
uv run git-slop plan --path README.md
uv run git-slop check
uv run git-slop version
uv run git-slop --help
```

## Install From Private Releases

Tagged releases publish wheel and source artifacts plus a release manifest. For
private `uv` installation, download the wheel and manifest with GitHub CLI,
verify the manifest SHA256, then install the wheel:

```bash
gh release download v0.7.1 --repo coreycoto/git-slop --pattern 'git_slop-*.whl' --pattern release-manifest.json --dir .artifacts/git-slop
shasum -a 256 .artifacts/git-slop/git_slop-0.7.1-py3-none-any.whl
uv tool install --force .artifacts/git-slop/git_slop-0.7.1-py3-none-any.whl
git-slop version
```

On macOS, the private Homebrew tap is the preferred operator install:

```bash
brew tap coreycoto/tap git@github.com:coreycoto/homebrew-tap.git
brew install coreycoto/tap/git-slop
git-slop version
```

The Codex plugin is published from this repo as
`git-slop@git-slop-marketplace`. It owns install/update, report-running,
interpretation, maintenance-planning, and consumer-adoption guidance.

## Command Surface

- `git slop init`
- `git slop find`
- `git slop show`
- `git slop explain`
- `git slop plan`
- `git slop check`
- `git slop version`

The package exposes both:

- `git-slop ...`
- `python -m git_slop ...`

### Explain and Plan

- `git slop explain`
  - explains one file, folder, cluster, relationship, or the current top-N
    hotspots from an existing schema-3 report
  - keeps hotspot cost and overlay evidence separate
- `git slop plan`
  - proposes bounded maintenance slices from one file, folder, cluster, or
    relationship selector
  - keeps the selected anchor first, prefers direct relationship slices over
    broader cluster-derived spill, and suppresses weaker subset proposals that
    add no new evidence
  - stays stdout-only in the first implementation
  - never mutates code, GitHub, or detector truth

Examples:

```bash
uv run git-slop explain --path src/git_slop/reporting.py
uv run git-slop explain --path src/git_slop
uv run git-slop explain --top 5
uv run git-slop plan --path src/git_slop
uv run git-slop plan --relationship duplicate_neighborhood-1234
```

## Generated State

Git Slop writes generated artifacts under `.slop/`:

```text
.slop/
  config.yaml
  .gitignore
  latest/
  runs/
  cache/
```

`find` writes:

- `.slop/latest/report.json`
- `.slop/latest/report.yaml`
- `.slop/latest/summary.md`
- `.slop/runs/<timestamp>/...`

The report timestamp reflects the analyzed repo snapshot so repeated runs on the
same HEAD can stay byte-identical.

## Report Contract

`report.json` is the machine contract. Current machine schema:

- `schema_version: 3`

Canonical top-level sections:

- `summary`
- `repo`
- `config`
- `stats`
- `files`
- `folders`
- `action_queue`
- `costs`
- `overlays`

Canonical stable cost sections:

- `costs.load`
- `costs.volatility`
- `costs.coordination`

Canonical overlay sections:

- `overlays.organization_health`
- `overlays.verification`
- `overlays.navigation`
- `overlays.blast_radius`
- `overlays.stewardship`
- `overlays.semantic_drift`

For one compatibility release cycle, Git Slop still emits these deprecated
mirrors:

- `organization_metrics`
- `relationships`
- `clusters`

## Config Contract

`.slop/config.yaml` now writes:

- `schema_version: 2`

Git Slop still accepts `schema_version: 1` configs and auto-normalizes them
forward for one compatibility cycle.

Current config namespaces:

- `inventory`
- `tokenization`
- `history`
- `scoring`
- `organization`
- `verification`
- `navigation`
- `blast_radius`
- `stewardship`
- `semantic_drift`
- `check`

Important defaults:

- organization-health stays always-on
- no user-facing overlay enable/disable switch
- deterministic candidate limiting is allowed internally for performance
- `history.follow_renames: true` remains opt-in

## Internal Layout

The detector now uses explicit internal layers:

```text
src/git_slop/
  cli/
  core/
  costs/
  graphs/
  reports/
  scoring/
  integrations/
```

Roles:

- `core/`: repository facts, config, cache, token facts, history facts, pipeline
- `costs/`: stable cost analyzers and overlay analyzers
- `graphs/`: co-change, similarity, relationships, and cluster helpers
- `reports/`: schema shaping, markdown, terminal, and bundle writing
- `scoring/`: stable hotspot scoring only
- `integrations/`: maintainer-only or detector-adjacent integrations

## What Git Slop Does Not Do

- rewrite code automatically
- require hosted APIs
- send repo data anywhere
- use an LLM for scoring
- claim a boundary is “wrong” without human review
- fold overlays into `priority_score`

## Roadmap Position

This repo has moved beyond pure detector work. The current downstream surfaces
now are:

- `git slop explain`
- `git slop plan`

Those commands consume the detector contract as-is. They do not rescore
hotspots, change `check`, or fold overlays into `priority_score`.

## Project Docs

- [Vision](docs/vision.md)
- [Architecture](docs/architecture.md)
- [Scoring Model](docs/scoring-model.md)
- [Roadmap](docs/roadmap.md)
- [Backlog Project Config](config/github/README.md)
- [Label Palette Config](config/labels/README.md)
- [Codex Runtime](.codex/README.md)
