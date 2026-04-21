---
name: "intake"
description: "Use this skill when repo-local notes, markdown, or DOCX research should be reconciled into backlog-ready apply artifacts."
---

# Intake

Use this skill when local research should produce the minimum evidence-backed
backlog reconciliation artifacts.

## Read First

- `docs/engineering/backlog-governance.md`
- `docs/engineering/github-mutation-workflow.md`
- `../_shared/references/github-mutation-contract.md`

## Workflow

1. Resolve repo context with `repo`.
2. Run `digest` on the local markdown or DOCX input.
3. Run `snapshot` to compare against the current backlog contract.
4. Keep bulky evidence in `.artifacts/research-intake/...`.
