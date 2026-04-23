---
name: "plan-quarter-apply"
description: "Use this skill when a reviewed quarter plan should be validated, turned into a milestone delta, and materialized as an apply report."
---

# Plan Quarter Apply

Use this skill to validate a reviewed quarter plan, build the milestone delta,
and materialize the apply report.

## Prerequisites

- Local interactive use requires both the official GitHub Codex plugin and the bundled GitHub connector mapping.
- Run `python3 ../../scripts/preflight_github_surface.py` if you need to confirm the combined local prerequisite before continuing.

## Read First

- `../_shared/references/github-runtime-prerequisites.md`
- `../_shared/references/backlog-project-contract.md`
- `../_shared/references/github-mutation-contract.md`
- `../_shared/references/workflow-tooling-surface.md`

## Workflow

1. Resolve the current repository context and backlog snapshot with `uv run agent-tools github current-repo --format json` and `uv run agent-tools github project-snapshot --format json`.
2. Validate the reviewed quarter plan with `uv run agent-tools github validate-quarter-plan --format json`.
3. Build the milestone delta with `uv run agent-tools github build-quarter-plan-delta --format json`.
4. Materialize the apply report with `uv run agent-tools github apply-quarter-plan-delta --format json`.
