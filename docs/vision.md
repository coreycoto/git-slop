# Vision

## Problem

Modern repositories can be technically readable and still be expensive to work
in.

Large files cost too much context to load. Old files have had plenty of time to
be split but still remain oversized. High-churn files amplify risk because they
keep drawing edits into already-expensive surfaces.

Git Slop exists to make that cost visible.

## Product Thesis

Git Slop should become a deterministic, local-first detector for AI-era repos.
It should answer one question extremely well:

> Which files cost too much context to load, reason about, and safely change?

The initial wedge is straightforward:

- treat token cost as the primary complexity signal
- use Git history for age and churn
- keep scoring deterministic and explainable
- emit machine-readable outputs for humans, CI, and agents

## Non-Goals

Git Slop is not:

- a hosted telemetry product
- an autonomous refactoring system
- a replacement for tests
- a replacement for human judgment
- an LLM-based scoring engine

## Success Criteria

A successful v1 lets a maintainer answer these questions in minutes:

- Which files cost the most context?
- Which of those are old enough to be worrying?
- Which of those are volatile enough to deserve immediate attention?
- What is the next safest refactor target?

A successful v2 lets an agent answer:

- What is the likely smell?
- What seam should I extract first?
- How do I verify that the refactor improved the repo?

A successful v3 lets a team show:

- detector output before
- a bounded plan
- a bounded refactor
- tests
- detector output after

## Release Shape

Git Slop has three deliberately separate phases:

- `v1-detector`: local-first hotspot detection and reporting
- `v2-explainer-planner`: optional narration and bounded refactor planning
- `v3-agentic-loop`: bounded execution, verification, and richer integrations

This separation is intentional. Trust comes from detector quality first. The
automation layers only make sense once the scoring model is stable.

## Core Principles

- Git is the source of truth for repository inventory and history.
- The detector stays deterministic and explainable.
- `context_band` and `priority_band` remain separate signals.
- JSON is the machine contract; Markdown is the human summary.
- The default workflow remains local-first and offline-friendly.

## Thesis

Readable code can still be slop.

If the file is too large to load cheaply, too old to excuse, and too volatile
to ignore, it is expensive even if it is beautifully formatted.
