# Git Slop Plugin

This plugin is the Codex guidance surface for the `git-slop` product CLI.

It covers:

- installing and updating the CLI
- running report, explain, plan, and check commands
- interpreting `.slop/latest/` artifacts
- preserving `.slop/` generated-state boundaries
- planning bounded maintenance work from hotspot evidence
- adopting `git-slop` in consumer repositories

It intentionally does not own generic backlog, release, project, or governance
workflows. When a reviewed `git-slop plan` should become backlog work, use the
separate `project-management-workflows` plugin from `coreycoto/agent-plugins`.

Product guidance should treat `.slop/latest/`, `.slop/runs/`, `.slop/cache/`,
prompt packs, SARIF exports, and plan/preview JSON as generated artifacts unless
a repository intentionally curates them as fixtures outside `.slop/`.
