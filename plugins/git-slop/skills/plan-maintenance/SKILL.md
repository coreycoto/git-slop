---
name: plan-maintenance
description: Turn reviewed git-slop findings into bounded maintenance slices without duplicating shared project-management workflows.
---

# Plan Maintenance From Git Slop

Use this skill when a reviewed hotspot should become a bounded maintenance
proposal.

## Workflow

1. Start from an existing report under `.slop/latest/`.
2. Run `git-slop explain` for the selected file, folder, cluster, or relationship.
3. Run `git-slop plan --format json` for the same selector when the output may
   become backlog work.
4. Keep the proposal narrow and evidence-backed.
5. Treat plan slice scope, out-of-scope paths, and evidence summary as human
   review guidance. Do not treat the plan as a patch or autonomous refactor loop.
   Do not treat overlay evidence as a rescore of `slop_score` or `slop_band`.
6. Use the plan payload's preview-only `backlog_handoff` metadata as input to
   `$project-management-workflows:plan-to-backlog-preview`.
7. Keep plan JSON local or uploaded as a review artifact unless the repository
   intentionally curates it as a fixture outside `.slop/`.
8. If local model summarization is useful, add `--prompt-pack <dir>` and use the
   generated prompt pack locally. Do not treat model output as detector truth.
9. Do not create, update, close, label, or milestone GitHub issues from
   `git-slop`; live mutation remains outside this product-specific skill.
