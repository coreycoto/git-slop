---
name: "review-to-backlog-preview"
description: "Use this skill when deterministic repo review findings should be previewed as backlog-ready issues without live GitHub mutation."
---

# Review To Backlog Preview

Use this skill to turn reviewed deterministic findings into backlog preview
artifacts without applying anything live.

## Prerequisites

- Local interactive use requires both the official GitHub Codex plugin and the bundled GitHub connector mapping.
- Run `python3 ../../scripts/preflight_github_surface.py` if you need to confirm the combined local prerequisite before continuing.

## Read First

- `../_shared/references/github-runtime-prerequisites.md`
- `../_shared/references/backlog-project-contract.md`
- `../_shared/references/github-mutation-contract.md`
- `../_shared/references/review-triage.md`
- `../_shared/references/workflow-tooling-surface.md`

## Workflow

1. Resolve the current repository context, backlog snapshot, and issue graph with `uv run agent-tools github current-repo --format json`, `uv run agent-tools github project-snapshot --format json`, and `uv run agent-tools github issue-graph --format json`.
2. Inspect the deterministic review findings payload.
3. Build the preview delta with `uv run agent-tools github review-to-backlog --format json`.
4. Keep the preview delta in `.artifacts/review-to-backlog/...`.
