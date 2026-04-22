# `git slop explain` / `git slop plan` Design

Date: 2026-04-22

This document defines the next product program after the detector evolution and
rollout wave.

Sequence:

1. `git slop explain`
2. `git slop plan`

Both commands consume existing detector output. Neither command may mutate
detector truth, rescore hotspots, or silently fold overlays into
`priority_score`.

## Product boundary

### `git slop explain`

Purpose:

- turn one schema-3 detector report into a bounded explanation of why a file,
  folder, relationship, or cluster is expensive
- keep hotspot cost and overlay evidence clearly separated

### `git slop plan`

Purpose:

- consume `explain`-ready evidence and propose bounded maintenance slices
- suggest reviewable work packages without applying changes or mutating GitHub

### Non-goals

- no detector rescoring
- no hidden overlay weighting
- no code mutation
- no GitHub mutation
- no mandatory LLM dependency
- no hosted-only workflow

## CLI surface

### `git slop explain`

Primary forms:

- `git slop explain --report .slop/latest/report.json --path <repo-path>`
- `git slop explain --report .slop/latest/report.json --cluster <cluster-id>`
- `git slop explain --report .slop/latest/report.json --relationship <relationship-id>`
- `git slop explain --report .slop/latest/report.json --top <N>`

Options to support in the first implementation:

- `--report <path>`
- one selector only:
  - `--path <repo-path>`
  - `--cluster <cluster-id>`
  - `--relationship <relationship-id>`
  - `--top <N>`
- `--format text|json`

Default behavior:

- if no selector is provided, behave as `--top 5`
- `--format text` is the human default

### `git slop plan`

Primary forms:

- `git slop plan --report .slop/latest/report.json --path <repo-path>`
- `git slop plan --report .slop/latest/report.json --cluster <cluster-id>`
- `git slop plan --report .slop/latest/report.json --relationship <relationship-id>`

Options to support in the first implementation:

- `--report <path>`
- exactly one selector:
  - `--path <repo-path>`
  - `--cluster <cluster-id>`
  - `--relationship <relationship-id>`
- `--max-slices <N>`
- `--format text|json`

Default behavior:

- `--max-slices 3`
- `--format text`

## Inputs

Required input:

- schema-3 `report.json`

Optional input:

- repo-local `.slop/config.yaml` only for contextual naming or future defaults
  lookup

Consumed detector sections:

- `files`
- `folders`
- `action_queue`
- `costs`
- `overlays`
- compatibility mirrors are ignored when canonical nested data is present

`explain` and `plan` must read canonical nested sections first:

- `files[].costs`
- `files[].overlays`
- `folders[].costs`
- `folders[].overlays`
- `overlays.organization_health`
- `overlays.verification`
- `overlays.navigation`
- `overlays.blast_radius`
- `overlays.stewardship`
- `overlays.semantic_drift`

## Output shape

### Human text mode

`git slop explain` text output:

- one concise header naming the selected target
- one section for hotspot cost:
  - load
  - volatility
  - coordination
- one section for overlay evidence:
  - organization health
  - verification
  - navigation
  - blast radius
  - stewardship
  - semantic drift
- one section for strongest supporting relationships or clusters
- one short “interpretation boundary” line stating that the output is evidence,
  not correctness proof

`git slop plan` text output:

- one concise header naming the selected target
- 1 to `max-slices` bounded maintenance slices
- each slice includes:
  - target scope
  - why it is grouped
  - which report evidence supports it
  - what should stay out of scope
- one short boundary line stating that the plan is a proposal, not an applied
  mutation

### Machine JSON mode

`git slop explain` JSON output:

- `schema_version`
- `report_schema_version`
- `command: "explain"`
- `selector`
- `target`
- `cost_summary`
- `overlay_summary`
- `supporting_relationships`
- `supporting_clusters`
- `boundary_note`

`git slop plan` JSON output:

- `schema_version`
- `report_schema_version`
- `command: "plan"`
- `selector`
- `target`
- `proposed_slices`
- `ranking_basis`
- `boundary_note`

## Ranking and selection rules

### `explain`

For `--top <N>`:

- start from `action_queue`
- keep the existing queue order unchanged
- explanations must describe why the current detector ranked items as it did;
  they must not rerank them

Supporting evidence selection:

- prefer relationships and clusters that directly reference the selected target
- sort by detector evidence strength, then stable id/path order
- cap each evidence list to a deterministic maximum of 5 items

### `plan`

Planning slices are grouped from existing evidence only.

Grouping priority:

1. selected hotspot path or cluster members
2. strongest direct relationships tied to the selection
3. tightly-scoped neighboring files only when they already appear in the same
   cluster or relationship evidence

Slice construction rules:

- each slice must be reviewable and bounded
- do not produce slices larger than 5 files in the first implementation
- do not merge unrelated overlay findings into one “mega-plan”
- prefer slices that align with existing clusters or direct relationships over
  folder-wide sweeps

## Interpretation rules

- `priority_score` continues to mean context cost only
- overlay evidence is parallel and explanatory
- `explain` may say a concept appears duplicated, scattered, volatile, weakly
  verified, or tightly coupled
- `explain` may not claim a boundary is wrong or a refactor is mandatory
- `plan` may propose maintenance slices
- `plan` may not imply safety or correctness guarantees

## Implementation guidance

- implement `explain` first
- reuse the existing schema-3 detector models rather than reparsing through
  deprecated mirrors
- keep the initial implementation local-first and deterministic
- add tests for:
  - path explanation
  - cluster explanation
  - relationship explanation
  - top-N explanation order
  - bounded slice construction
  - JSON/text output stability
