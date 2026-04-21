---
name: "plan-quarter-preview"
description: "Use this skill when a reviewed quarter plan should be validated and previewed as a milestone assignment delta without live mutation."
---

# Quarter Plan Preview

Use this skill to validate a quarter plan and preview the resulting milestone
assignment delta.

## Read First

- `docs/engineering/backlog-governance.md`
- `../_shared/references/backlog-project-contract.md`

## Workflow

1. Resolve `repo`.
2. Run `snapshot`.
3. Run `validate-quarter-plan`.
4. Run `build-quarter-delta`.
