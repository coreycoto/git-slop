---
name: run-report
description: Run git-slop report, explain, plan, and check commands in a repository while preserving its local artifact and warn-only conventions.
---

# Run Git Slop Reports

Use this skill when the user wants a fresh `git-slop` report or a targeted
explanation/plan.

## Commands

- Initialize repo state when needed: `git-slop init`
- Generate artifacts: `git-slop find`
- Explain current evidence: `git-slop explain --top 5` or `git-slop explain --path <path>`
- Propose bounded work: `git-slop plan --path <path>` or `git-slop plan --relationship <id>`
- Compare existing reports: `git-slop compare --base <old-report.json> --head <new-report.json>`
- Export SARIF locally: `git-slop sarif --report <report.json> --output <path.sarif>`
- Run the gate surface: `git-slop check`

For editor-adjacent workflows, point maintainers at
`docs/plans/editor-artifact-consumption-recipes.md` and keep SARIF consumption
local and explicit.

Consumer repos may provide a wrapper such as `./scripts/git_slop.sh`; prefer it
when present because it may enforce the repo's pinned install contract.
