# Roadmap

## Current State

Git Slop has shipped the detector foundation and the first downstream
maintenance surfaces:

- schema-3 detector reports with explicit `costs` and `overlays`
- config schema 2 with migration from schema 1
- stable hotspot scoring and `git slop check`
- always-on overlay analyzers
- local caching for history and token facts
- `git slop explain`
- `git slop plan`
- prompt-pack-only local model handoff
- preview-only backlog handoff metadata
- advanced read-only trend comparison with `git slop compare`
- advanced read-only SARIF 2.1.0 export with `git slop sarif`

The next work should bias toward adoption, documentation clarity, consumer
contracts, and narrow improvements backed by real repository evidence.

## Stable Scope

The stable detector scope remains:

- load and context cost
- age and volatility
- coordination cost
- hotspot ranking
- action-queue generation
- CI checks

These signals must stay explainable and must not silently absorb overlay
pressure.

## Overlay Scope

Current overlay families:

- organization health
- verification
- navigation
- blast radius
- stewardship
- semantic drift

Overlays remain evidence-first and always-on. They can support explanations,
plans, and SARIF properties, but they do not redefine `priority_score`,
`priority_band`, `context_band`, or `git slop check`.

## Downstream Surfaces

Current downstream surfaces consume existing detector artifacts:

- `explain` consumes a schema-3 report
- `plan` consumes a schema-3 report
- `compare` consumes two schema-3 reports
- `sarif` consumes one schema-3 report

These commands remain read-only with respect to source code, GitHub, scoring,
and check semantics. Prompt packs are deterministic local files for advisory
summarization only.

## Explicitly Out Of Scope

- hosted SaaS
- autonomous refactoring
- first-party editor extensions
- language servers
- background detectors
- hidden score inflation from overlays
- new `check` thresholds for overlays
- LLM-backed detector scoring

Editor-adjacent artifact consumption is now tracked as low-priority future work
in [Issue #39](https://github.com/coreycoto/git-slop/issues/39), rather than as
checked-in planning documents.

## Delivery Guidance

Recommended sequence:

1. keep active-repo adoption moving
2. keep release and consumer contracts easy to verify
3. refine `explain` and `plan` quality without changing detector meaning
4. add new integrations only when they can consume existing artifacts without
   background mutation
5. touch detector-core only when multi-repo evidence justifies it
