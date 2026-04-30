# Vision

## Problem

Modern repositories can be technically readable and still be expensive to work
in.

Large files cost too much context to load. Old files linger as oversized
surfaces. High-churn files keep pulling edits into already expensive areas.

And even when no single file is huge, one idea can still be costly when it is:

- duplicated
- scattered
- weakly bounded
- hard to verify
- hard to find
- forced to co-change across the repo

Git Slop exists to make that cost visible.

## Product Thesis

Git Slop should stay a deterministic, local-first detector for AI-era repos.

It should answer one stable question extremely well:

> Which files cost too much context to load, reason about, and safely change?

And it should answer a second question in a separate evidence layer:

> Which concepts cost too much coordination because they are duplicated,
> scattered, weakly verified, difficult to navigate, or forced to co-change?

The separation is the thesis:

- context cost is the stable detector contract
- overlays are structural and operational evidence
- later explainer/planner surfaces may consume that evidence
- explainer/planner surfaces must not own or mutate detector truth

## Non-Goals

Git Slop is not:

- a hosted telemetry product
- an autonomous refactoring system
- a behavioral safety oracle
- a replacement for human judgment
- an LLM-based scoring engine

## Success Criteria

A successful detector lets a maintainer answer:

- Which files cost the most context?
- Which of those are old enough to be worrying?
- Which of those are volatile enough to deserve immediate attention?
- Which of those have high coordination or verification risk?
- What is the next bounded refactor target?

A successful detector-refinement wave also lets a maintainer inspect:

- duplicate or near-duplicate knowledge
- suspicious co-change pairs
- scattered concept clusters
- cross-boundary leakage
- weak verification adjacency
- high blast-radius files
- ownership concentration
- term drift across roots

## Core Principles

- Git is the source of truth for repository inventory and history.
- The detector stays deterministic and explainable.
- `context_band` and `priority_band` remain separate signals.
- Overlay evidence remains separate from `priority_score`.
- JSON is the machine contract; Markdown is the human summary.
- The default workflow remains local-first and offline-friendly.
- Structural and operational findings are evidence, not semantic proof.
