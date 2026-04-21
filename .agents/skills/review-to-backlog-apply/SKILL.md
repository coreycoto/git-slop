---
name: "review-to-backlog-apply"
description: "Use this skill when deterministic repo review findings should be turned into backlog apply artifacts for a reviewed GitHub follow-up pass."
---

# Review To Backlog Apply

Use this skill to convert structured review findings into a reviewed backlog
delta and a follow-on apply report.

## Read First

- `docs/engineering/backlog-governance.md`
- `docs/engineering/github-mutation-workflow.md`
- `../_shared/references/review-triage.md`

## Workflow

1. Resolve `repo`.
2. Inspect `snapshot` and `graph`.
3. Run `review-to-backlog`.
4. Run `apply-review-delta` after review.
