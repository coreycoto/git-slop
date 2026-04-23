---
name: "plan-quarter-preview"
description: "Use this skill when a reviewed quarter plan should be validated and previewed as a milestone assignment delta without live mutation."
---

# Plan Quarter Preview

Use this skill to validate a quarter plan and preview the resulting milestone
delta without applying it live.

## Prerequisites

- Local interactive use expects the official GitHub Codex plugin.
- Run `python3 ../../scripts/preflight_github_plugin.py` if you need to confirm the local prerequisite.

## Read First

- `../_shared/references/backlog-project-contract.md`
- `../_shared/references/github-mutation-contract.md`
- `../_shared/references/workflow-tooling-surface.md`

## Workflow

1. Resolve the current repository and backlog snapshot with `uv run agent-tools github current-repo --format json` and `uv run agent-tools github project-snapshot --format json`.
2. Validate the quarter plan payload with `uv run agent-tools github validate-quarter-plan --format json`.
3. Build the milestone delta preview with `uv run agent-tools github build-quarter-plan-delta --format json`.
4. Keep the preview artifacts in `.artifacts/quarter-plan/...`.
