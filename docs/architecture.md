# Architecture

## Design Goals

The architecture should stay deterministic, inspectable, and boring enough to
run locally without mystery.

The scaling rule is simple:

- `core/` gathers facts
- `costs/` interprets facts
- `graphs/` builds relationship structures
- `scoring/` owns stable hotspot scoring
- `reports/` renders output
- `integrations/` keeps detector-adjacent extras out of the core path

## Runtime Pipeline

```text
Git repository
  -> inventory facts
  -> token facts
  -> history facts
  -> stable hotspot scoring
  -> overlay analyzers
  -> report assembly
  -> bundle writing
  -> CLI / CI / agent-readable outputs
```

## Internal Layout

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

### `core/`

`core/` is the fact-gathering layer.

Important modules:

- `core/config.py`
- `core/repository.py`
- `core/inventory.py`
- `core/token_facts.py`
- `core/history_facts.py`
- `core/cache.py`
- `core/models.py`
- `core/pipeline.py`

Responsibilities:

- repo root resolution
- tracked-file inventory
- binary/decode filtering
- config normalization and migration
- context-token facts
- structural-token facts
- Git history mining
- cache-key construction
- typed fact objects

### `costs/`

`costs/` owns analyzers.

Stable cost analyzers:

- `LoadCostAnalyzer`
- `VolatilityCostAnalyzer`
- `CoordinationCostAnalyzer`

Always-on overlay analyzers:

- `OrganizationHealthAnalyzer`
- `VerificationOverlayAnalyzer`
- `NavigationOverlayAnalyzer`
- `BlastRadiusOverlayAnalyzer`
- `StewardshipOverlayAnalyzer`
- `SemanticDriftOverlayAnalyzer`

### `graphs/`

`graphs/` builds reusable relationship structures:

- co-change graph
- token-similarity helpers
- relationship selectors
- cluster selectors

### `reports/`

`reports/` owns schema shaping and human surfaces:

- machine report assembly
- Markdown summary
- terminal rendering
- bundle writing
- explain, plan, compare, and SARIF payload rendering

### `integrations/`

Maintainer-only detector-adjacent code lives under `integrations/`, not the
core detector pipeline.

## Facts Model

Typed pipeline objects now include:

- `RepositoryFacts`
- `InventoryFacts`
- `FileFacts`
- `TokenFacts`
- `HistoryFacts`
- `ChangeSetFacts`
- `BaselineFacts`
- `HotspotScore`
- `OverlayFinding`
- `Relationship`
- `Cluster`

Analyzers consume facts. They should not shell out to Git or re-tokenize files
independently.

## Token Pipelines

Git Slop now keeps two token systems:

### Context tokens

- `tiktoken`-aligned
- used for load and context-band math

### Structural tokens

Deterministic lexical/path normalization:

- lowercase
- camelCase and snake_case splitting
- number normalization
- quoted-string normalization
- path-segment normalization

These structural tokens drive duplication, cohesion, navigation, and drift
analysis.

## Report Contract

Current machine report:

- `schema_version: 3`

Canonical top-level shape:

- `summary`
- `repo`
- `config`
- `stats`
- `files`
- `folders`
- `action_queue`
- `costs`
- `overlays`

Canonical stable cost blocks:

- `costs.load`
- `costs.volatility`
- `costs.coordination`

Canonical overlay blocks:

- `overlays.organization_health`
- `overlays.verification`
- `overlays.navigation`
- `overlays.blast_radius`
- `overlays.stewardship`
- `overlays.semantic_drift`

For one compatibility cycle, Git Slop also emits:

- `organization_metrics`
- `relationships`
- `clusters`

`git slop check` ignores overlays entirely.

## Config Contract

`.slop/config.yaml` now writes:

- `schema_version: 2`

Current namespaces:

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

Legacy `schema_version: 1` configs are still auto-normalized for one
compatibility cycle.

## Caching

Required cache namespaces:

- `.slop/cache/history/`
- `.slop/cache/tokens/context/`
- `.slop/cache/tokens/structural/`
- `.slop/cache/organization-health/`

Rules:

- cache is never required for correctness
- stale cache is ignored automatically
- cold and warm runs on the same HEAD/config should be byte-identical
- candidate limiting must be deterministic

## CLI Surface

The CLI exposes a core detector workflow and read-only advanced artifact
commands:

- `git slop init`
- `git slop find`
- `git slop show`
- `git slop explain`
- `git slop plan`
- `git slop check`
- `git slop compare`
- `git slop sarif`
- `git slop version`

Command boundaries:

- `find` runs the detector and writes `.slop/latest/` plus `.slop/runs/`.
- `show`, `explain`, `plan`, `check`, and `sarif` consume an existing schema-3
  report.
- `compare` consumes two existing schema-3 reports.
- Prompt packs are explicit local outputs from `explain` and `plan`.

Downstream commands do not rescore detector truth, change `check` semantics, or
mutate GitHub.
