---
name: interpret-results
description: Interpret git-slop report artifacts and explain which findings are observational, actionable, or suitable for backlog planning.
---

# Interpret Git Slop Results

Use this skill when reviewing `.slop/latest/report.json`,
`.slop/latest/report.yaml`, `.slop/latest/summary.md`, or
`.slop/latest/health.md`.

## Interpretation Rules

- Treat `git-slop` as a detector and evidence surface, not a correctness oracle.
- Keep hotspot costs separate from overlay evidence.
- Use `slop_score`, `slop_band`, and `context_band` for the stable
  hotspot queue.
- Use health file/folder bands, distributions, watchlists, and concentration
  metrics as human-facing rollups of existing facts, not as a new score.
- Use overlays for supporting evidence about organization, verification,
  navigation, blast radius, stewardship, and semantic drift.
- Follow a health finding's deterministic `next_command` to inspect the exact
  path before proposing work.
- Prefer a targeted `git-slop explain` before turning a finding into a work item.
- Treat `refactor_required` as a threshold label and review candidate, not an
  automatic mandate to change code.
- Treat `.slop/latest/`, `.slop/runs/`, `.slop/cache/`, SARIF, prompt packs,
  plan JSON, and compare JSON as generated evidence. Reference or upload them
  when useful; do not commit routine generated output.
