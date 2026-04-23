---
name: "intake-preview"
description: "Use this skill when repo-local notes, markdown, or DOCX research should be normalized into backlog preview artifacts without live GitHub mutation."
---

# Intake Preview

Use this skill to turn local research material into preview-only backlog
artifacts.

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
2. Normalize the local research input with `uv run agent-tools research digest --format json`.
3. Compare the normalized research against the current backlog state.
4. Keep the preview artifacts in `.artifacts/intake/...`.
