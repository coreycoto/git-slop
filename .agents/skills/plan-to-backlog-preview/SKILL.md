---
name: "plan-to-backlog-preview"
description: "Use this skill when reviewed git-slop plan output should be previewed as backlog-ready maintenance issues without live GitHub mutation."
---

# Plan To Backlog Preview

Use this skill to convert reviewed `git slop plan` output into deterministic
backlog preview artifacts without applying anything live.

## Read First

- `docs/engineering/backlog-governance.md`
- `docs/engineering/github-mutation-workflow.md`
- `../_shared/references/backlog-project-contract.md`

## Workflow

1. Resolve `repo`.
2. Inspect `snapshot` so the target backlog context is current.
3. Run `plan-to-backlog` with the reviewed plan JSON, matching report JSON, and
   an explicit epic.
4. Keep the preview delta in `.artifacts/plan-to-backlog/...`.
