---
name: "github-backlog-mutate"
description: "Use this skill when a reviewed mixed backlog mutation plan should be validated and turned into a deterministic apply report."
---

# GitHub Backlog Mutate

Use this skill when a reviewed issue-centric backlog mutation plan spans issues,
labels, milestones, or project fields.

## Read First

- `docs/engineering/github-mutation-workflow.md`
- `../_shared/references/github-mutation-contract.md`

## Workflow

1. Resolve `repo`.
2. Run `validate-backlog-mutations`.
3. Run `apply-backlog-mutations`.
