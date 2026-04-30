# Roadmap

## Current State

Git Slop now has:

- a detector CLI
- schema-3 report output with explicit `costs` and `overlays`
- config schema 2 with migration from schema 1
- stable hotspot scoring
- always-on overlay analyzers
- local caching for history and token facts
- `git slop explain`
- `git slop plan`
- maintainer-facing `plan`-to-backlog preview integration
- prompt-pack-only local model handoff
- preview-only backlog handoff metadata
- read-only trend comparison with `git slop compare`
- read-only SARIF export with `git slop sarif`
- preview-only bounded refactor handoff with `git slop refactor-preview`
- JSON, YAML, Markdown, and terminal reporting

The detector bootstrap program is complete. The repo is now past the first
explainer/planner release and into the phase where the shipped surfaces should
be tightened, adopted, and selectively extended.

## Completed Baseline

The following are now shipped baselines rather than roadmap proposals:

1. typed analyzer architecture
2. explicit `costs` / `overlays` report contract
3. expanded always-on overlay evidence
4. `explain` and `plan` on top of the detector report
5. preview-first maintainer backlog handoff for `plan`
6. prompt-pack local model handoff without runtime model invocation
7. additive V2 explain/plan payloads for evidence summaries and backlog preview

That means the next work should bias toward:

- adoption
- product-shape refinement
- narrow fixes when cross-repo evidence justifies them

Not toward another immediate detector-core rewrite.

## Stable Detector Scope

Stable hotspot scope remains:

- load / context cost
- age / volatility
- hotspot ranking
- queue generation
- CI checks

Those stay easy to explain and must not silently absorb overlay pressures.

## Overlay Scope

Current overlay families:

- organization health
- verification
- navigation
- blast radius
- stewardship
- semantic drift

These remain evidence-first and always-on.

## Completed Program: V2 Follow-Through

`git slop explain` and `git slop plan` are now shipped with additive V2 payload
fields, prompt-pack local model handoff, and preview-only backlog handoff
metadata. Future V2 work should be narrow adoption follow-through, not
another feature wave.

Completed V2 follow-through scope:

- explanation quality and coverage on real repos
- plan slice quality, ranking, and evidence summarization
- maintainer-facing backlog handoff quality
- optional local model support only as prompt packs downstream of detector truth
- richer maintainer-only agent hooks built on the existing report contract and
  preview-only backlog metadata

These surfaces should continue to consume detector evidence; they should not
redefine it.

## Completed Program: V3 Agentic Loop

V3 begins with read-only report comparison before higher-risk integrations.
Completed V3 slices:

- trend comparisons with `git slop compare`
- SARIF output with `git slop sarif`
- preview-only bounded refactor loops with `git slop refactor-preview`
- editor-facing integration research
- editor-adjacent artifact consumption recipes

`git slop compare` consumes two existing schema-3 reports and describes file,
folder, overlay, and queue movement. It does not rerun the detector, mutate
`.slop/`, imply causality, or change scoring/check semantics.

`git slop sarif` exports action-queue findings from an existing schema-3 report
as SARIF 2.1.0. It preserves hotspot cost and overlay evidence as separate
properties and does not upload results, rerun the detector, or mutate GitHub.

`git slop refactor-preview` consumes saved `git slop plan --format json` output
and emits bounded maintainer steps, review checklist items, evidence, and
non-mutating patch-preview notes. It does not edit files, generate diffs, invoke
models, commit or push changes, rerun the detector, rescore detector truth, or
mutate GitHub.

Editor-facing integration research recommends deferring a first-party editor
extension and starting with documented consumption of SARIF and JSON artifacts.
Static artifact recipes now document how editor-adjacent workflows can consume
SARIF, plan JSON, and refactor-preview JSON without adding an extension,
language server, watcher, hosted service, or model runtime. See
[Editor Integration Research](editor-integration-research.md) and
[Editor Artifact Consumption Recipes](editor-artifact-consumption-recipes.md).

## Explicit Not Yet

Still out of scope for this program:

- hosted SaaS
- autonomous refactoring
- editor plugins
- hidden score inflation from overlays
- new `check` thresholds for overlays
- LLM-backed detector scoring

## Delivery Guidance

Recommended future sequence:

1. keep active-repo adoption moving
2. refine `explain` and `plan` quality without changing detector meaning
3. expand maintainer-only preview workflows carefully and explicitly
4. touch detector-core only when multi-repo evidence requires it
5. defer autonomous and editor-facing loops until compare and SARIF foundations exist
