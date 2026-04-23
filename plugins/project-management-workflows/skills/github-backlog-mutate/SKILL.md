---
name: "github-backlog-mutate"
description: "Use this skill when a reviewed issue-centric backlog mutation plan should be validated and materialized as an apply report."
---

# GitHub Backlog Mutate

Use this skill to validate a reviewed issue-centric backlog mutation plan and
materialize the apply report for controlled GitHub writes.

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
2. Validate the reviewed mutation plan with `uv run agent-tools github validate-backlog-mutations --format json`.
3. Materialize the apply report with `uv run agent-tools github apply-backlog-mutations --format json`.
4. Verify the live GitHub state immediately after the apply path completes.
