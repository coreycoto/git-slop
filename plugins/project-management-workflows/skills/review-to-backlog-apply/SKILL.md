---
name: "review-to-backlog-apply"
description: "Use this skill when deterministic repo review findings should become reviewed backlog deltas and apply reports."
---

# Review To Backlog Apply

Use this skill to turn deterministic findings into backlog deltas and materialize
the apply report after the preview has been reviewed.

## Prerequisites

- Local interactive use expects the official GitHub Codex plugin.
- Run `python3 ../../scripts/preflight_github_plugin.py` if you need to confirm the local prerequisite.

## Read First

- `../_shared/references/backlog-project-contract.md`
- `../_shared/references/github-mutation-contract.md`
- `../_shared/references/review-triage.md`
- `../_shared/references/workflow-tooling-surface.md`

## Workflow

1. Resolve the current repository context, backlog snapshot, and issue graph with `uv run agent-tools github current-repo --format json`, `uv run agent-tools github project-snapshot --format json`, and `uv run agent-tools github issue-graph --format json`.
2. Inspect the deterministic review findings payload.
3. Build the review backlog delta with `uv run agent-tools github review-to-backlog --format json`.
4. Materialize the apply report with `uv run agent-tools github apply-review-backlog-delta --format json` only after the delta is reviewed.
