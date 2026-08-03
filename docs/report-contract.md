# Report And Config Contract

`report.json` is Git Slop's versioned machine contract. `report.yaml`,
`summary.md`, `health.md`, terminal output, SARIF, and GitHub annotations are
projections of that data.

## Detector Report

The current report schema is:

- `schema_version: 4`

Canonical top-level fields and sections are:

- `schema_version`
- `generated_at`
- `analyzed_revision_at`
- `summary`
- `repo`
- `config`
- `stats`
- `files`
- `folders`
- `action_queue`
- `costs`
- `overlays`
- `health`

`generated_at` records when the detector ran.
`analyzed_revision_at` records the analyzed HEAD commit timestamp when one is
available. Repository provenance, including branch, HEAD SHA, remote URL,
shallow status, and repository root, lives under `repo`.

### Stable Costs

Canonical cost sections are:

- `costs.load`
- `costs.volatility`
- `costs.coordination`

Each stable cost section carries `analysis_status: stable` and
`analysis_version: 1`. Per-file coordination evidence includes weighted
co-change PageRank as `cochange_pagerank`; load evidence uses direct-parent
folder token totals and repository-wide token concentration. These v1 fields
retain the pre-Rust formulas and types.

The stable detector fields are:

- `slop_score`
- `slop_band`
- `context_band`
- `action_queue`

`slop_score` is a deterministic maintenance-pressure score, not an overall
quality score. `git slop check` evaluates file bands from an existing report
against configured or explicit thresholds.

### Additive Overlays

Canonical overlay sections are:

- `overlays.organization_health`
- `overlays.verification`
- `overlays.navigation`
- `overlays.blast_radius`
- `overlays.stewardship`
- `overlays.semantic_drift`

Overlay evidence explains adjacent structural and operational pressure. It does
not change stable scoring or `check` behavior.

Every overlay wrapper carries `analysis_status` and `analysis_version`.
`overlays.semantic_drift` also always carries a `findings` array, including when
there are no findings.

The Rust organization analyzer is `analysis_version: 2`. Its deterministic
candidate bound and graph construction differ from organization analysis v1,
so overlay relationship IDs, cluster IDs, and counts are comparable only when
their `analysis_version` values match. IDs within v2 remain deterministic and
use the established NUL-delimited BLAKE2b identifier encoding.

The canonical `relationships` object always carries `analysis_status`,
`analysis_version`, and these arrays:

- `duplicate_neighborhoods`
- `near_duplicate_neighborhoods`
- `temporal_coupling_edges`
- `lexical_affinity_edges`
- `boundary_leakage_edges`

The canonical `clusters` object always carries `analysis_status`,
`analysis_version`, and these arrays:

- `duplicate_sets`
- `scattered_concepts`
- `boundary_leakage_clusters`
- `consolidation_candidates`

All canonical arrays remain present when empty. Relationship and cluster
records retain the exact `kind` value associated with their canonical section.

For one compatibility cycle, reports also emit these top-level mirrors:

- `organization_metrics`
- `relationships`
- `clusters`

New consumers should prefer the canonical `costs` and `overlays` sections.

### Repository Health

Schema 4 now contains an additive `health` section with:

- `file_band_counts`
- `folder_band_counts`
- `file_distribution`
- `folder_distribution`
- `profile_rollups`
- `language_rollups`
- `refactor_candidates`
- `watchlist`
- `findings`

Distribution records include count, total, p50, p90, p95, p99, maximum, and
top-1/top-5/top-10 concentration shares. Findings contain path, severity,
human-readable reason, stable detector fields, and a deterministic
`next_command`.

Health file bands are a human-facing projection of context bands:

- `compact`
- `healthy`
- `warning`
- `refactor_required`

Folder health bands use direct child-file counts and direct token totals from
the `health.folder_bands` config. They do not alter file-level stable scoring.

## Bundle Contract

Each successful `find` writes:

```text
.slop/latest/
  report.json
  report.yaml
  summary.md
  health.md
```

The same four files are written to one timestamped directory under
`.slop/runs/`.

- `report.json` is the canonical automation format.
- `report.yaml` contains the equivalent schema-4 payload.
- `summary.md` preserves the detailed detector and overlay view.
- `health.md` presents status bands, distributions, review candidates,
  watchlists, actionable findings, and compact rollups for humans and CI.

Consumers must use `schema_version`, not the Markdown layout, as the machine
compatibility boundary.

## Downstream Payloads

Downstream commands consume existing reports and emit additive payloads:

- `git slop explain`: schema-v2 explain payload
- `git slop plan`: schema-v2 plan payload
- `git slop compare`: schema-v1 comparison of two schema-4 reports
- `git slop sarif`: SARIF 2.1.0 projection of one schema-4 report
- `git slop health --format json`: JSON projection of the additive health data

`git slop health --format markdown` regenerates the human dashboard.
`git slop health --format github` emits a bounded set of workflow-command
annotations and accepts `--max-annotations`.

These commands do not rerun the detector, rescore detector truth, or change
`check` semantics. `explain` and `plan` write local prompt packs only when the
caller explicitly supplies `--prompt-pack`; `sarif` writes a file only when the
caller supplies `--output`.

## Config

`.slop/config.yaml` uses:

- `schema_version: 2`

Current config namespaces are:

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
- `health`
- `check`

Important defaults:

- context tokenization uses `cl100k_base`
- organization-health and other overlay evidence remain always on
- file context bands use 3,072, 8,000, and 10,000 token boundaries
- folder health bands use direct tokens and direct file counts
- `check.fail_on_context_band: critical`
- `check.fail_on_slop_band: critical`
- deterministic candidate limiting is allowed internally for performance
- `history.follow_renames: false`

Git Slop accepts legacy schema-1 config files and normalizes them forward in
memory for one compatibility cycle. `git slop init` writes schema 2.

## Compatibility Rules

- Additive fields may appear inside schema 4.
- Removing or retyping accepted fields requires a schema-version change.
- Rendered Markdown may evolve without changing the report schema.
- Overlay and health additions may not silently alter stable scoring or check
  thresholds.
- Unknown additive fields should be ignored by consumers.
- Repositories should keep their generated report bundles untracked unless
  they are deliberately curated fixtures.
