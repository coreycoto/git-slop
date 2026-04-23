# GitHub Backlog Config

This directory holds the repo-owned backlog and project overlay for `git-slop`.

Use the reusable workflow contract from the plugin for mutation, preview/apply,
and artifact rules:

- `plugins/project-management-workflows/skills/_shared/references/backlog-project-contract.md`
- `plugins/project-management-workflows/skills/_shared/references/github-mutation-contract.md`
- `plugins/project-management-workflows/skills/_shared/references/review-triage.md`
- `plugins/project-management-workflows/skills/_shared/references/workflow-tooling-surface.md`

## Files

- `project_config.json`: canonical GitHub Project identity, fields, and views
- `issue_seed_catalog.json`: repo-owned issue seed catalog for roadmap epics and queue seeds

## Local Overlay

`git-slop` uses:

- GitHub Project `git-slop`
- Project fields:
  - `Status`
  - `Priority`
  - `Queue Order`
- native parent/sub-issue relationships
- quarter-focused milestones

Issue taxonomy for this repo:

- `Epic:`
- `Research:`
- `Enhancement:`
- `Bug:`
- `Maintenance:`

Current roadmap epics are seeded from `issue_seed_catalog.json`:

- `Epic: V1 detector`
- `Epic: V2 explainer and planner`
- `Epic: V3 agentic loop`

Issue forms should stay aligned with that local taxonomy.
