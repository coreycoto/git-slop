---
name: "ensure-quarter-milestones"
description: "Use this skill when the current and next git-slop quarter milestones need to be validated against policy."
---

# Ensure Quarter Milestones

Use this skill to compute the current and next quarter milestone contract and
check a supplied live snapshot for drift.

## Read First

- `docs/engineering/backlog-governance.md`
- `../_shared/references/backlog-project-contract.md`

## Workflow

1. Resolve `repo`.
2. Run `milestone-check`.
3. Keep any drift evidence in `.artifacts/github-governance/...`.
