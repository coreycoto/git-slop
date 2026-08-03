---
name: run-report
description: Run git-slop report, health, explain, plan, and check commands while preserving local artifact and advisory CI conventions.
---

# Run Git Slop Reports

Use this skill when the user wants a fresh `git-slop` report or a targeted
explanation/plan.

## Commands

- Initialize repo state when needed: `git-slop init`
- Generate artifacts: `git-slop find`
- Render the human health dashboard: `git-slop health`
- Render a selected report: `git-slop health --report <report.json>`
- Emit the health payload for automation: `git-slop health --format json`
- Emit bounded CI annotations: `git-slop health --format github --max-annotations 10`
- Explain current evidence: `git-slop explain --top 5` or `git-slop explain --path <path>`
- Propose bounded work: `git-slop plan --path <path>` or `git-slop plan --relationship <id>`
- Compare existing reports: `git-slop compare --base <old-report.json> --head <new-report.json>`
- Export SARIF locally: `git-slop sarif --report <report.json> --output <path.sarif>`
- Run the gate surface: `git-slop check`

One successful `find` writes `report.json`, `report.yaml`, `summary.md`, and
`health.md` to both `.slop/latest/` and a timestamped `.slop/runs/` directory.
`health` and the other downstream commands consume those reports. `health`
writes its selected rendering to stdout; it does not rewrite `health.md` or
rerun `find`. Health findings are advisory and a successful rendering exits
zero. Use `check` when the repository explicitly wants an enforcement gate.

Read [the health command reference](references/health.md) when selecting a
format, interpreting health evidence, or reproducing the GitHub Action flow.

Generated `.slop/latest/`, `.slop/runs/`, `.slop/cache/`, prompt packs, SARIF
exports, plan JSON, and compare JSON should stay untracked unless a repository
intentionally curates examples or fixtures outside the runtime `.slop/` tree.
Upload a bounded generated artifact when review needs a durable copy. Prefer
`health.md` alone; add `report.json` only when automation needs schema-4 data.

Consumer repos may provide a wrapper such as `./scripts/git_slop.sh`; prefer it
when present because it may enforce the repo's pinned install contract.
