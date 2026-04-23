---
name: "label-palette-design"
description: "Use this skill when the checked-in GitHub label palette should be previewed or refreshed without broad label churn."
---

# Label Palette Design

Use this skill to preview or refresh the deterministic label palette for the
current repository.

## Prerequisites

- Local interactive use expects the official GitHub Codex plugin.
- Run `python3 ../../scripts/preflight_github_plugin.py` if you need to confirm the local prerequisite.

## Read First

- `../_shared/references/label-palette-contract.md`
- `../_shared/references/github-mutation-contract.md`
- `../_shared/references/workflow-tooling-surface.md`

## Workflow

1. Resolve the current repository context with `uv run agent-tools github current-repo --format json`.
2. Validate the checked-in palette with `uv run agent-tools github sync-label-palette --check --format json`.
3. Preview the resulting delta before any apply path runs.
4. Keep the evidence in `.artifacts/github-governance/...`.
