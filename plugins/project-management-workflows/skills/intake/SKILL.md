---
name: "intake"
description: "Use this skill when repo-local notes, markdown, or DOCX research should be normalized and turned into the minimum reviewed GitHub backlog changes."
---

# Intake

Use this skill to convert local research into deterministic backlog artifacts and
reviewed apply-ready GitHub mutations.

## Prerequisites

- Local interactive use expects the official GitHub Codex plugin.
- Run `python3 ../../scripts/preflight_github_plugin.py` if you need to confirm the local prerequisite.

## Read First

- `../_shared/references/backlog-project-contract.md`
- `../_shared/references/github-mutation-contract.md`
- `../_shared/references/workflow-tooling-surface.md`

## Workflow

1. Resolve the current repository and backlog snapshot with `uv run agent-tools github current-repo --format json` and `uv run agent-tools github project-snapshot --format json`.
2. Normalize the local research input with `uv run agent-tools research digest --format json`.
3. Build the smallest reviewed mutation plan with `uv run agent-tools github validate-backlog-mutations --format json`.
4. Keep preview artifacts in `.artifacts/intake/...` before any apply step.
