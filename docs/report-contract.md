# Report And Config Contract

`report.json` is Git Slop's versioned machine contract. Optional `report.yaml`,
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
- `scope`
- `config`
- `stats`
- `files`
- `folders`
- `action_queue`
- `costs`
- `overlays`
- `health`
- `collection_metadata`
- `diagnostics`

`generated_at` records when the detector ran.
`analyzed_revision_at` records the analyzed HEAD commit timestamp when one is
available. Repository provenance, including branch, HEAD SHA, remote URL,
shallow status, and repository root, lives under `repo`.

`scope` records its mode, normalized repo-relative path, selected path count,
and SHA-256 selected-path digest. Baseline compatibility includes repository,
analyzer, tokenizer, configuration, history completeness, and scope identity.
Forced comparisons retain exact base and head mismatch values.

Canonical file records include `content_fingerprint`, preventing history-only
movement from being mistaken for source changes. Canonical arrays are complete;
`collection_metadata` records `total`, `returned`, `limit`, and `truncated`.

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

`slop_score` and `slop_band` are deterministic maintenance-pressure evidence,
not overall quality scores. `context_band` is the separate file context/load
classification. `git slop check` evaluates those file bands from an existing
report against configured or explicit thresholds.

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

Health file bands are a human-facing projection of context/load bands:

- `compact`
- `healthy`
- `warning`
- `refactor_required`

Folder health bands use direct child-file counts and direct token totals from
the `health.folder_bands` config. They do not alter file-level stable scoring.

For every warning or refactor-required folder surfaced in Markdown, the
projection names each direct metric that crossed the boundary for the displayed
band. Warning rows compare against the configured healthy ceilings, for example
`19 direct files > 17 healthy ceiling` or
`128,001 direct tokens > 128,000 healthy ceiling`. Refactor-required rows
compare against the warning ceilings. When both direct metrics cross the
relevant ceiling, the two clauses are joined with `; `.

Each surfaced folder also receives a copyable
`git-slop explain --path <folder>/` next command; the repository root uses
`git-slop explain --path .`. Its bounded preview contains exactly one recursive
`agent_context` descendant, ranked deterministically by descending
`slop_score`, descending tokens, and ascending path. The preview reports that
descendant's maintenance-pressure band and score separately from its
context/load band and token count. This selection does not change detector
ordering, scores, thresholds, or the action queue.

Rendered finding severity is a third, presentation-level concept:

- `notice` maps to the GitHub `::notice` workflow command.
- `warning` maps to `::warning`.
- `error` maps to `::error`.

The mapping is one-to-one for recognized health severities and does not depend
on advisory versus enforcing Action policy. A defensive unknown-severity
fallback may emit `warning`; it does not redefine the three supported levels.
`--max-annotations` bounds the ordered stream without changing the retained
findings' levels.

Health Markdown uses one locale-independent number policy:

- integer counts and token totals use comma grouping;
- non-integral percentiles use comma grouping and exactly two decimal places;
- concentration and profile shares use exactly one decimal place plus `%`;
- maintenance-pressure scores use exactly one decimal place.

These strings are projection-only. JSON retains its existing numeric values
and types, and the folder guidance and formatting changes do not change schema
4.

## Bundle Contract

Each successful `find` writes:

```text
.slop/latest/
  report.json
  report.yaml  # only when output.yaml is true
  summary.md
  health.md
```

The same four files are written to one timestamped directory under
`.slop/runs/`.

- `report.json` is the canonical automation format.
- `report.yaml` contains the equivalent schema-4 payload only when explicitly enabled.
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

All three `health` formats write to standard output. They do not rewrite
`.slop/latest/health.md` or any timestamped report bundle; `find` is the command
that persists `health.md`. Health findings are advisory, so successful
rendering exits 0 even when findings are present. Use `git slop check` to apply
the stable threshold gate.

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

The `health` namespace has these schema-2 defaults:

| Key | Default | Purpose |
| --- | ---: | --- |
| `health.data_context_min_bytes` | `262144` | Size threshold used with supported data extensions when assigning the `data_context` profile |
| `health.folder_bands.compact_max_direct_tokens` | `31999` | Inclusive compact direct-token ceiling |
| `health.folder_bands.healthy_max_direct_tokens` | `128000` | Inclusive healthy direct-token ceiling |
| `health.folder_bands.warning_max_direct_tokens` | `256000` | Inclusive warning direct-token ceiling |
| `health.folder_bands.warning_max_direct_files` | `17` | Inclusive direct-file ceiling before warning |
| `health.folder_bands.refactor_required_max_direct_files` | `37` | Inclusive direct-file ceiling before `refactor_required` |
| `health.summary_top_files` | `10` | File rows retained in rendered dashboard sections |
| `health.summary_top_folders` | `10` | Folder rows retained in rendered dashboard sections |

File health bands project the existing
`tokenization.context_bands.compact_max_tokens: 3072`,
`healthy_max_tokens: 8000`, and `warning_max_tokens: 10000` defaults. Folder
bands use direct `agent_context` tokens and files: values above the healthy
token or warning file ceiling are `warning`; values above the warning token or
refactor-required file ceiling are `refactor_required`.

Important defaults:

- context tokenization uses `cl100k_base`
- organization-health and other overlay evidence remain always on
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
