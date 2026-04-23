---
name: "docs-taxonomy"
description: "Use this skill when docs, skills, plugin references, or agent guidance should be moved back into the intended taxonomy with narrow docs-only edits."
---

# Docs Taxonomy

Use this skill to detect taxonomy drift across docs, skills, plugin guidance,
and custom-agent surfaces, then prepare the smallest docs-only fix.

## Prerequisites

- Local interactive use requires both the official GitHub Codex plugin and the bundled GitHub connector mapping.
- Run `python3 ../../scripts/preflight_github_surface.py` if you need to confirm the combined local prerequisite before continuing.

## Read First

- `../_shared/references/github-runtime-prerequisites.md`
- `../_shared/references/agent-decision-patterns.md`
- `../_shared/references/workflow-tooling-surface.md`

## Workflow

1. Validate the Codex and metadata surface with `uv run python scripts/validate_codex_surface.py`.
2. Inspect taxonomy drift across docs, plugin guidance, and agent configuration.
3. Make the smallest docs-only changes needed to restore the intended layering.
4. Use standard `gh pr create` or `gh pr edit` for the narrow docs PR and keep evidence in `.artifacts/docs-taxonomy/...`.
