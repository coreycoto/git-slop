---
name: "ensure-quarter-milestones"
description: "Use this skill when the current and next quarter milestone policy should be checked against the repository backlog contract."
---

# Ensure Quarter Milestones

Use this skill to validate the current and next quarter milestone policy against
the current backlog contract.

## Prerequisites

- Local interactive use expects the official GitHub Codex plugin.
- Run `python3 ../../scripts/preflight_github_plugin.py` if you need to confirm the local prerequisite.

## Read First

- `../_shared/references/backlog-project-contract.md`
- `../_shared/references/github-mutation-contract.md`
- `../_shared/references/workflow-tooling-surface.md`

## Workflow

1. Resolve the current repository context with `uv run agent-tools github current-repo --format json`.
2. Inspect the checked-in project contract and current milestone state.
3. Build the deterministic milestone-check artifact with `uv run agent-tools github milestone-check --format json`.
4. Keep the resulting evidence in `.artifacts/quarter-plan/...` when a write is not yet approved.
