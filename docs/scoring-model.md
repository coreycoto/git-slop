# Scoring Model

## Purpose

Git Slop now distinguishes two categories of cost:

- **stable hotspot costs**
- **overlay evidence**

Stable hotspot costs drive:

- `slop_score`
- `slop_band`
- `context_band`
- `git slop check`

Overlay evidence does not.

## Stable Hotspot Costs

### `context_band`

`context_band` answers:

> How expensive is this file to load into working context?

Default bands:

| Token range | Context band | Meaning |
| --- | --- | --- |
| `0-3072` | `compact` | Cheap to load |
| `3073-8000` | `healthy` | Preferred operating band |
| `8001-10000` | `warning` | Expensive but still workable |
| `>10000` | `critical` | Too costly for routine context loading |

### `slop_band`

`slop_band` answers:

> What deterministic maintenance-pressure band does this file fall into?

Default mapping:

| Score | Slop band |
| --- | --- |
| `<50` | `low` |
| `50-64` | `moderate` |
| `65-84` | `high` |
| `>=85` | `critical` |

## Stable Pressure Components

The stable detector still uses:

- `context_pressure`
- `age_pressure`
- `churn_pressure`

Default weighting:

```text
slop_score =
  100 * (
    0.60 * context_pressure +
    0.20 * age_pressure +
    0.20 * churn_pressure
  )
```

This remains the stable hotspot contract. `slop_score` is a deterministic
maintenance-pressure score from stable costs, not an overall correctness or
quality score.

## Stable Cost Outputs

### Load

Load uses **context tokens** only.

Required outputs:

- `file_token_count`
- `folder_token_count`
- `top_file_share`
- `top_3_file_share`
- `token_concentration_ratio`
- `context_band`
- `load_pressure`

### Volatility

Volatility uses Git history plus token deltas.

Required outputs:

- `commit_count_window`
- `recency_weighted_commits`
- `line_churn_window`
- `token_churn_window`
- `relative_token_churn`
- `late_churn_spike`
- `volatility_pressure`

### Coordination

Coordination uses changesets and co-change, but still stays outside the stable
score until an explicit scoring decision is made.

Git Slop emits these as stable explicit costs in the report:

- `files_touched_per_change`
- `folders_touched_per_change`
- `edit_hunks_per_change`
- `cochange_degree`
- `cochange_centrality`
- `cross_folder_cochange_ratio`
- `change_diffusion`
- `coordination_pressure`

The hotspot queue still means context cost. Coordination evidence is exposed in
the report; it does not silently inflate `slop_score`.

## Structural Context / Organization Health

Git Slop now emits a parallel organization-health model for coordination cost.

That layer looks for deterministic structural evidence such as:

- duplicated token neighborhoods
- near-duplicate knowledge
- high-diffusion commits
- temporal coupling edges
- lexical affinity across boundaries
- likely consolidation candidates

Canonical report location:

- `overlays.organization_health`

Compatibility mirrors for one release cycle:

- `organization_metrics`
- `relationships`
- `clusters`

### Coordination Pressures

Current organization-health pressures:

- `duplication_pressure`
- `fragmentation_pressure`
- `cohesion_pressure`
- `boundary_pressure`

Git Slop also preserves the earlier organization-health file overlay signals:

- `duplication_pressure`
- `diffusion_pressure`
- `coupling_pressure`
- `boundary_pressure`

These remain repo-relative evidence. They are not stable proof that a boundary
is wrong.

### Explicit Non-Goals

The organization-health layer is:

- not a cleanliness oracle
- not a fourth weight in `slop_score`
- not an LLM-based judgment system

## Additional Always-On Overlays

Canonical overlay namespaces:

- `overlays.verification`
- `overlays.navigation`
- `overlays.blast_radius`
- `overlays.stewardship`
- `overlays.semantic_drift`

These are all evidence-oriented and remain outside `slop_score`.

### Verification

Signals:

- `test_adjacency_score`
- `nearby_test_paths` (actual test modules/files only)
- `nearby_verification_paths` (workflows and validation scripts, labeled
  separately and never treated as test adjacency)
- `test_cochange_ratio`
- `hotspot_without_nearby_tests`
- `churn_without_test_churn`
- `verification_gap`

### Navigation

Signals:

- `path_depth`
- `sibling_count`
- `folder_width`
- `search_ambiguity`
- `term_dispersion`
- `duplicate_name_count`
- `navigation_pressure`

### Blast radius

Signals:

- `cochange_degree`
- `weighted_cochange_degree`
- `cochange_pagerank`
- `cross_folder_coupling`
- `average_changeset_size_when_touched`
- `blast_radius_pressure`

### Stewardship

Signals:

- `author_count_window`
- `author_entropy`
- `top_author_share`
- `days_since_non_bot_edit`
- `recent_maintainer_diversity`
- `stewardship_pressure`

### Semantic drift

Signals:

- token neighborhood vectors by root
- drift findings for high-signal terms
- per-file drift pressure

This layer is explicitly experimental and evidence-first.

## Model Rules

- `context_band` remains a raw size signal.
- `slop_band` remains a composite maintenance-pressure signal.
- Overlay evidence remains separate from `slop_score` and must not redefine
  `slop_band`, `context_band`, or `git slop check` semantics.
- `git slop check` ignores overlays entirely.
- LLMs must not mutate detector scores.
- Thresholds and weights remain adjustable defaults, not sacred constants.
