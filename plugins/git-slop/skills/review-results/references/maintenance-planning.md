# Maintenance Planning From Reviewed Evidence

Use this workflow only after the user selects one reviewed file, folder,
cluster, or relationship for a bounded maintenance proposal.

1. Confirm the candidate against `.slop/latest/health.md` and its threshold,
   distribution, and concentration context.
2. Run `git-slop explain` for the exact selector if the current review did not
   already do so.
3. Run `git-slop plan --format json` for that same selector.
4. Keep the proposal narrow and evidence-backed. Preserve its explicit scope,
   out-of-scope paths, and evidence summary.
5. Treat the plan as human review guidance, not as a patch or autonomous
   refactor loop. Do not use overlay evidence to rescore `slop_score` or
   `slop_band`, and do not treat health bands as a second detector gate.
6. Keep plan JSON local or upload it as a bounded review artifact unless the
   repository intentionally curates it as a fixture outside `.slop/`.
7. If local model summarization is useful, add `--prompt-pack <dir>` and use the
   generated prompt pack locally. Do not treat model output as detector truth.
8. Hand the plan payload's preview-only `backlog_handoff` metadata to an
   independently installed project-management workflow only when the user asks
   for backlog preparation.
9. Do not create, update, close, label, or milestone GitHub issues from this
   skill. Live tracker mutation remains outside the Git Slop product plugin.
