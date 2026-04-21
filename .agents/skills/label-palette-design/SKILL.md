---
name: "label-palette-design"
description: "Use this skill when the checked-in git-slop label palette should be previewed or refreshed without broad label churn."
---

# Label Palette Design

Use this skill to validate the checked-in label palette and preview the
repo-managed subset.

## Read First

- `docs/engineering/issue-label-palette.md`
- `../_shared/references/github-mutation-contract.md`

## Workflow

1. Resolve `repo`.
2. Run `label-palette`.
3. Keep live GitHub changes manual-first.
