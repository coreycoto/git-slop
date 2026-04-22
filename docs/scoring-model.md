# Scoring Model

## Purpose

Git Slop scores two different things:

- raw context cost
- refactor urgency

And now, in a separate experimental layer, it also measures:

- coordination cost

Those are related, but they are not the same.

## Two-Band Model

### `context_band`

`context_band` answers:

> How expensive is this file to load into working context?

Initial default token bands:

| Token range | Context band | Meaning |
| --- | --- | --- |
| `0-3072` | `compact` | Cheap to load |
| `3073-8000` | `healthy` | Preferred operating band |
| `8001-10000` | `warning` | Expensive but still workable |
| `>10000` | `critical` | Too costly for routine context loading |

### `priority_band`

`priority_band` answers:

> How urgently should this file be refactored?

Initial default mapping:

| Score | Priority band |
| --- | --- |
| `<50` | `watchlist` |
| `50-64` | `needs_refactor` |
| `65-84` | `should_refactor` |
| `>=85` | `must_refactor` |

## Pressure Components

The v1 detector uses these signals:

- `context_pressure`
- `age_pressure`
- `churn_pressure`

Initial default weighting:

```text
priority_score =
  100 * (
    0.60 * context_pressure +
    0.20 * age_pressure +
    0.20 * churn_pressure
  )
```

These are v1 defaults, not permanent law. They should remain easy to explain
and easy to tune once real detector output exists.

## Age Model

Default age function:

```text
age_pressure = 1 - 2^(-age_days / age_half_life_days)
```

Initial default:

- `age_half_life_days = 180`

That makes older files increasingly suspicious without instantly penalizing new
work-in-progress files.

## Churn Model

Trailing-window metrics:

- `revisions_window`
- `added_window`
- `deleted_window`
- `churn_lines_window`
- `relative_churn_window`

Default normalization:

```text
revision_norm = min(1.0, revisions_window / p95_revisions_window)
relative_churn_norm = min(1.0, relative_churn_window / p95_relative_churn_window)
churn_pressure = 0.6 * revision_norm + 0.4 * relative_churn_norm
```

This keeps change frequency and relative churn in the same model while reducing
bias from file size and commit style.

## Reason Codes

Planned reason codes:

- `high_token_cost`
- `critical_token_cost`
- `old_file`
- `high_revision_frequency`
- `high_relative_churn`
- `old_and_volatile`

The reason-code layer exists so the score remains interpretable.

## Structural Context / Organization Health

Git Slop now emits a parallel organization-health model for coordination cost.

That layer looks for structural evidence such as:

- duplicated token neighborhoods
- near-duplicate knowledge
- high-diffusion commits
- temporal coupling edges
- lexical affinity across boundaries
- likely consolidation candidates

The first outputs live under these report namespaces:

- `organization_metrics`
- `relationships`
- `clusters`

They are intentionally evidence-oriented. They are not a fourth weight in the
main hotspot score.

### Coordination Pressures

The current experimental overlay emits these repo-relative signals:

- `duplication_pressure`
- `diffusion_pressure`
- `coupling_pressure`
- `boundary_pressure`

Those pressures are normalized relative to the current repo and recent history.
High values are suspicious because they deviate from local norms, not because
they violate a universal cleanliness law.

### Explicit Non-Goals

The organization-health layer is:

- not a cleanliness oracle
- not a fourth weight in `priority_score` yet
- not an LLM-based judgment system

It exists to make structural cost inspectable so later `explain` and `plan`
surfaces can consume concrete evidence instead of inventing their own opaque
heuristics.

## Model Rules

- `context_band` must remain a raw size signal.
- `priority_band` must remain a composite urgency signal.
- organization-health pressures must remain separate from `priority_score`.
- LLMs must not mutate detector scores.
- The scoring model should stay deterministic and auditable.
- Thresholds and weights are adjustable defaults, not sacred constants.
