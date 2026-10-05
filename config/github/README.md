# GitHub Backlog Config

This directory holds the repo-owned backlog and project overlay for `git-slop`.

Reusable policy guidance comes from the public `coreycoto/agent-plugins`
packages pinned in `.agents/plugins/marketplace-source.json`; the portable
`git-slop` Agent Plugin documents this repository's product workflow.

Use the public `$project-management:project-governance` and
`$project-management:backlog-planning` guidance for policy and review. Use
`$gh-steward:gh-steward-reviewed-governance` for native GitHub snapshots,
reviewed plans, exact-hash applies, and durable recovery.

Hosted callers extract signed plans with `gh steward plan extract` and register
the original file with `runs context-record-plan --input native-plan=FILE`.
The native tool owns digest validation and serialization so numeric values keep
their signed representation. These prepared callers require the corresponding
qualified gh-steward release and exact lock update before promotion.

The relevant reusable references are:

- backlog/project contract
- GitHub mutation contract
- review triage
- workflow tooling surface

## Files

- `project_config.json`: canonical GitHub Project identity, fields, and views
- `dogfood-regression-acceptances.json`: reviewed, base-bound Dogfood regression
  ceilings for intentionally broad changes
- `dogfood-regression-acceptances/<base-sha>.json`: isolated acceptance shards
  when a reviewed change should not be recorded in the root manifest itself

The verifier combines the root manifest with canonical SHA-named shards and
requires schema version 1 for every input and globally unique base SHAs. A
single root manifest remains supported. Dogfood acceptances are inert unless
their exact base SHA matches. Each entry is also bound to a path, content digest,
reason, non-critical severity, and maximum score. New paths, changed content,
worse scores, critical regressions, and stale base revisions fail closed. The
absolute repository policy still runs after an accepted comparison.

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
