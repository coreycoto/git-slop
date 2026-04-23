---
name: "plan-to-backlog-preview"
description: "Use this skill when reviewed plan output should be previewed as backlog-ready maintenance issues without live GitHub mutation."
---

# Plan To Backlog Preview

Use this skill to convert reviewed planning output into deterministic backlog
preview artifacts without applying anything live.

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
2. Inspect the reviewed plan JSON and matching report JSON.
3. Require an explicit epic and build the preview delta with `uv run agent-tools github plan-to-backlog --format json`.
4. Keep the preview preview-only: one backlog-ready `Maintenance:` issue per plan slice and no live GitHub mutation.
5. Keep the preview delta in `.artifacts/plan-to-backlog/...`.
