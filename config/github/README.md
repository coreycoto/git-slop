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
- `dogfood-regression-acceptances.json`: reviewed, base-bound Dogfood regression
  ceilings for intentionally broad changes

Dogfood acceptances are inert unless their exact base SHA matches. Each entry is
also bound to a path, content digest, reason, non-critical severity, and maximum
score. New paths, changed content, worse scores, critical regressions, and stale
base revisions fail closed. The absolute repository policy still runs after an
accepted comparison.

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
