---
name: "intake-preview"
description: "Use this skill when repo-local notes, markdown, or DOCX research should be normalized into backlog preview artifacts without live GitHub mutation."
---

# Intake Preview

Use this skill to normalize local research and preview backlog impact without
writing to GitHub.

## Read First

- `docs/engineering/backlog-governance.md`
- `../_shared/references/backlog-project-contract.md`
- `../_shared/references/agent-decision-rubric.md`

## Workflow

1. Resolve repo context with `repo`.
2. Run `digest` on the local markdown or DOCX input.
3. Run `snapshot` when backlog context is needed.
4. Keep the result in `.artifacts/research-intake/...`.
