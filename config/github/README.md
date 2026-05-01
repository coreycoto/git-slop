# GitHub Backlog Config

This directory holds the repo-owned backlog and project overlay for `git-slop`.

Use the reusable workflow contract from the installed
`project-management-workflows` plugin from `coreycoto/agent-plugins` for
mutation, preview/apply, and artifact rules.

The relevant shared references there are:

- backlog/project contract
- GitHub mutation contract
- review triage
- workflow tooling surface

## Files

- `project_config.json`: canonical GitHub Project identity, fields, and views

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

Issue forms should stay aligned with that local taxonomy. Historical roadmap
seed catalogs are intentionally not kept in the public repository.
