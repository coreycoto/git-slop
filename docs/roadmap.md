# Roadmap

## Current State

Git Slop now has:

- a detector CLI
- schema-3 report output with explicit `costs` and `overlays`
- config schema 2 with migration from schema 1
- stable hotspot scoring
- always-on overlay analyzers
- local caching for history and token facts
- JSON, YAML, Markdown, and terminal reporting

The detector program is not “done forever,” but it is now in the validation
and adoption phase rather than the bootstrap phase.

## Active Program: Detector Evolution

The current program includes three completed technical shifts:

1. typed analyzer architecture
2. explicit `costs` / `overlays` report contract
3. expanded always-on overlay evidence

That means the next work should bias toward:

- validation
- rollout
- narrow detector fixes when cross-repo evidence justifies them

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

## Deferred Program: Explain / Plan

The next major program after detector evolution is:

- `git slop explain`
- `git slop plan`

Those surfaces should consume detector evidence; they should not redefine it.

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

1. keep broader repo rollout moving
2. capture validation notes from real repos
3. fix detector-level issues only when they appear across repos
4. start `explain` / `plan` once detector output feels stable enough to trust
