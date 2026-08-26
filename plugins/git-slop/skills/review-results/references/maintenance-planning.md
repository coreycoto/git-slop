# Maintenance Planning From Reviewed Evidence

Use this workflow only after the user selects one reviewed file, folder,
cluster, or relationship for a bounded maintenance proposal.

| Disposition | Use when |
| --- | --- |
| `implement` | Evidence supports a bounded change with demonstrated value |
| `consolidate` | Existing surface can absorb the need while reducing duplication |
| `defer` | Evidence or timing is insufficient; record a concrete revisit trigger |
| `accept` | The cost is real but intentionally carried for a stated reason |
| `won't fix` | The work is outside product goals or costs more than its likely value |

1. Confirm the candidate against `.slop/latest/health.md` and its threshold,
   distribution, and concentration context.
2. Run `git-slop explain` for the exact selector if the current review did not
   already do so.
3. Assign the selected finding one disposition: `implement`, `consolidate`,
   `defer`, `accept`, or `won't fix`. A maintenance proposal is appropriate only
   for `implement` or `consolidate`; preserve the rationale for every other
   disposition and stop.
4. Run `git-slop plan --format json` for that same selector only after selecting
   `implement` or `consolidate`.
5. Keep the proposal narrow and evidence-backed. Preserve its explicit scope,
   out-of-scope paths, and evidence summary.
6. Treat the plan as human review guidance, not as a patch or autonomous
   refactor loop. Do not use overlay evidence to rescore `slop_score` or
   `slop_band`, and do not treat health bands as a second detector gate.
7. When the plan adds or promotes durable surface, read
   [the contract-admission reference](contract-admission.md), run its admission
   test, assign contract states, and include a multidimensional surface-area
   ledger. Prefer an internal seam or expiring experiment when evidence is not
   strong enough for a permanent promise.
8. Keep plan JSON local or upload it as a bounded review artifact unless the
   repository intentionally curates it as a fixture outside `.slop/`.
9. If local model summarization is useful, add `--prompt-pack <dir>` and use the
   generated prompt pack locally. Do not treat model output as detector truth.
10. Hand the plan payload's preview-only `backlog_handoff` metadata to an
   independently installed project-management workflow only when the user asks
   for backlog preparation.
11. Do not create, update, close, label, or milestone GitHub issues from this
   skill. Live tracker mutation remains outside the Git Slop product plugin.
