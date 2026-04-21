---
name: "plan-quarter-apply"
description: "Use this skill when a reviewed quarter plan should be validated, converted into a delta, and recorded as an apply report."
---

# Quarter Plan Apply

Use this skill when a quarter plan is ready to move from preview into a
reviewed apply report.

## Read First

- `docs/engineering/backlog-governance.md`
- `docs/engineering/github-mutation-workflow.md`

## Workflow

1. Resolve `repo`.
2. Run `snapshot`.
3. Run `validate-quarter-plan`.
4. Run `build-quarter-delta`.
5. Run `apply-quarter-delta`.
