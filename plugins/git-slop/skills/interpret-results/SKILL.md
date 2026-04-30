---
name: interpret-results
description: Interpret git-slop report artifacts and explain which findings are observational, actionable, or suitable for backlog planning.
---

# Interpret Git Slop Results

Use this skill when reviewing `.slop/latest/report.json`,
`.slop/latest/report.yaml`, or `.slop/latest/summary.md`.

## Interpretation Rules

- Treat `git-slop` as a detector and evidence surface, not a correctness oracle.
- Keep hotspot costs separate from overlay evidence.
- Use `priority_score`, `priority_band`, and `context_band` for the stable
  hotspot queue.
- Use overlays for supporting evidence about organization, verification,
  navigation, blast radius, stewardship, and semantic drift.
- Prefer a targeted `git-slop explain` before turning a finding into a work item.
- Treat `.slop/latest/`, `.slop/runs/`, `.slop/cache/`, SARIF, prompt packs,
  plan JSON, compare JSON, and refactor-preview JSON as generated evidence.
  Reference or upload them when useful; do not commit routine generated output.
