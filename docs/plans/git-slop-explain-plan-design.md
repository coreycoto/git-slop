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
- no model invocation from prompt-pack generation

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
- `--prompt-pack <dir>`

Default behavior:

- if no selector is provided, behave as `--top 5`
- `--format text` is the human default
- folder targets also emit:
  - `cost_summary.descendant_hotspots`
  - `overlay_summary.descendant_overlay_maxima`
- `--top <N>` uses compact per-hotspot blocks in text mode
- `--top <N>` prints the interpretation boundary once at the end

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
- `--prompt-pack <dir>`

Default behavior:

- `--max-slices 3`
- `--format text`
- schema-3 reports only
- primary output prints to stdout; prompt packs are explicit local file outputs

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
- folder targets also include:
  - top descendant hotspots from current detector order
  - descendant overlay maxima
- one section for overlay evidence:
  - organization health
  - verification
  - navigation
  - blast radius
  - stewardship
  - semantic drift
- one section for strongest supporting relationships or clusters
- `--top <N>` uses compact per-hotspot blocks instead of repeating full path
  explanations back-to-back
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
- `evidence_summary`
- `boundary_note`

Folder-target additive JSON fields:

- `cost_summary.descendant_hotspots`
- `overlay_summary.descendant_overlay_maxima`

`git slop plan` JSON output:

- `schema_version`
- `report_schema_version`
- `command: "plan"`
- `selector`
- `target`
- `proposed_slices`
- `ranking_basis`
- `backlog_handoff`
- `boundary_note`

Each proposed slice also includes `evidence_summary` and `backlog_handoff`.
Backlog handoff is preview/report-only. It may include proposed issue title,
maintenance issue type, suggested labels, priority hint, evidence summary,
acceptance criteria, and source selector metadata, but it must not mutate
GitHub.

Prompt-pack output writes deterministic local files when `--prompt-pack <dir>`
is provided: `context.json`, `prompt.md`, and `README.md`. Prompt packs are for
local model summarization only. Git Slop does not invoke models, configure
providers, or send repository data anywhere.

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
- keep the anchor slice first for every selector type
- for folder selectors, suppress weaker subset slices that add no new
  supporting relationship or cluster evidence
- for relationship selectors, keep spill-heavy shared clusters as supporting
  evidence only instead of turning them into follow-up slices
- for broad cluster selectors, start from the strongest direct
  relationship-backed pair when available before adding narrower follow-up
  slices

Ranking and suppression rules in the shipped implementation:

- rank by selector class first:
  1. anchor slice
  2. direct relationship slice
  3. compact cluster slice
  4. broad cluster-derived slice
- then rank deterministically by:
  - relationship support count
  - cluster support count
  - out-of-scope count
  - top-three in-scope priority-score sum
  - lexicographic scope path order
- merge identical-scope slices before ranking
- suppress any later slice whose scope is a strict subset of an already-ranked
  slice and that adds no new supporting relationship or cluster ids

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
