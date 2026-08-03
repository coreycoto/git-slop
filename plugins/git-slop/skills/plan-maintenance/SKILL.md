---
name: plan-maintenance
description: Turn reviewed git-slop findings into bounded maintenance slices without duplicating shared project-management workflows.
---

# Plan Maintenance From Git Slop

Use this skill when a reviewed hotspot should become a bounded maintenance
proposal.

## Workflow

1. Start from an existing report under `.slop/latest/`.
2. Review `.slop/latest/health.md` for the threshold, distribution, and
   concentration context behind a candidate.
3. Run `git-slop explain` for the selected file, folder, cluster, or relationship.
4. Run `git-slop plan --format json` for the same selector when the output may
   become backlog work.
5. Keep the proposal narrow and evidence-backed.
6. Treat plan slice scope, out-of-scope paths, and evidence summary as human
   review guidance. Do not treat the plan as a patch or autonomous refactor loop.
   Do not treat overlay evidence as a rescore of `slop_score` or `slop_band`.
   Treat health bands as rollups, not a second detector gate.
7. Use the plan payload's preview-only `backlog_handoff` metadata as input to
   `$project-management-workflows:plan-to-backlog-preview`.
8. Keep plan JSON local or uploaded as a review artifact unless the repository
   intentionally curates it as a fixture outside `.slop/`.
9. If local model summarization is useful, add `--prompt-pack <dir>` and use the
   generated prompt pack locally. Do not treat model output as detector truth.
10. Do not create, update, close, label, or milestone GitHub issues from
   `git-slop`; live mutation remains outside this product-specific skill.
