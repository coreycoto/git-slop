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

## Active Program: V2 Follow-Through

`git slop explain` and `git slop plan` are now shipped with additive V2 payload
fields, prompt-pack local model handoff, and preview-only backlog handoff
metadata. Remaining V2 work should be narrow adoption follow-through, not
another feature wave.

Current V2 follow-through scope:

- explanation quality and coverage on real repos
- plan slice quality, ranking, and evidence summarization
- maintainer-facing backlog handoff quality
- optional local model support only as prompt packs downstream of detector truth
- richer maintainer-only agent hooks built on the existing report contract and
  preview-only backlog metadata

These surfaces should continue to consume detector evidence; they should not
redefine it.

## Deferred Program: V3 Agentic Loop

Still deferred to V3:

- bounded refactor loops
- SARIF output
- trend comparisons
- editor-facing integrations

## Explicit Not Yet

Still out of scope for this program:

- hosted SaaS
- autonomous refactoring
- editor plugins
- hidden score inflation from overlays
- new `check` thresholds for overlays
- LLM-backed detector scoring

## Delivery Guidance

Recommended sequence from here:

1. keep active-repo adoption moving
2. refine `explain` and `plan` quality without changing detector meaning
3. expand maintainer-only preview workflows carefully and explicitly
4. touch detector-core only when multi-repo evidence requires it
5. defer autonomous and editor-facing loops to V3
