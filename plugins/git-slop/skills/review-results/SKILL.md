---
name: review-results
description: Review existing git-slop report and health artifacts, explain detector and overlay evidence, and optionally turn one explicitly selected finding into a bounded maintenance proposal. Use when a user asks what results mean, which findings are actionable, or wants a plan from a reviewed finding; do not use merely to generate a fresh report.
---

# Review Git Slop Results

Start from an existing `.slop/latest/report.json`, `report.yaml`, `summary.md`,
or `health.md` artifact.

## Interpret The Evidence

1. Treat `git-slop` as a detector and evidence surface, not a correctness
   oracle.
2. Keep hotspot costs separate from overlay evidence. Use `slop_score`,
   `slop_band`, and `context_band` for the stable hotspot queue.
3. Use health file/folder bands, distributions, watchlists, and concentration
   metrics as human-facing rollups of existing facts, not as a second score.
4. Treat health output as advisory: findings do not change the command's
   success status or the stable `check` gate.
5. Use overlays only as supporting evidence about organization, verification,
   navigation, blast radius, stewardship, and concept dispersion.
6. Follow a finding's deterministic `next_command`, or run a targeted
   `git-slop explain`, before recommending work.
7. Treat `budget_exceeded` as a threshold label and review candidate, not an
   automatic mandate to change code.
8. Stop after explaining and prioritizing the evidence unless the user
   explicitly asks for a maintenance proposal.

## Plan One Reviewed Finding

When the user explicitly selects a finding for planning, read
[the maintenance-planning reference](references/maintenance-planning.md) and
follow it for that finding only.

## Preserve Generated-State Boundaries

Treat `.slop/latest/`, `.slop/runs/`, `.slop/cache/`, SARIF, prompt packs, plan
JSON, and compare JSON as generated evidence. Reference or upload a bounded
artifact when useful; do not commit routine generated output.
