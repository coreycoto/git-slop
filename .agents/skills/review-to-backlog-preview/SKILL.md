---
name: "review-to-backlog-preview"
description: "Use this skill when deterministic repo review findings should be turned into backlog-ready preview artifacts without live GitHub mutation."
---

# Review To Backlog Preview

Use this skill to convert structured review findings into a deterministic issue
delta without applying anything live.

## Read First

- `docs/engineering/backlog-governance.md`
- `../_shared/references/review-triage.md`

## Workflow

1. Resolve `repo`.
2. Inspect `snapshot` and `graph` as needed.
3. Run `review-to-backlog` with the findings payload.
4. Keep the delta in `.artifacts/review-to-backlog/...`.
