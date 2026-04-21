# Backlog Governance

- Status: current
- Audience: maintainer
- Canonical: yes

This document defines the stable governance rules for `git-slop` backlog
tracking in GitHub. It is the durable policy reference for issue taxonomy,
labels, milestones, project usage, issue forms, and agent-facing backlog work.

The checked-in machine-readable backlog project identity lives in
`config/github/project_config.json`. GitHub remains the live source of truth
for backlog state and history.

## Source Of Truth

Treat the following as the source of truth for backlog state:

- GitHub Project `git-slop`
- Project fields:
  - `Status`
  - `Priority`
  - `Queue Order`
- native GitHub parent and sub-issue relationships
- native GitHub milestones

Use issue body prose for context only. Do not use prose as the hard source of
truth when native GitHub structure already exists.

## Issue Taxonomy

Use these title prefixes consistently:

- `Epic:` multi-issue workstream such as a roadmap phase
- `Research:` decision, evaluation, or scoped uncertainty-reduction work
- `Enhancement:` net-new or intentionally expanded behavior
- `Bug:` broken shipped behavior, regressions, or docs-versus-behavior mismatch
- `Maintenance:` refactors, dependency upgrades, CI, developer experience, cleanup

`git-slop` does not use `Initiative:` in this first governance pass. Epics are
the highest issue level.

### Epic

Use `Epic:` for a multi-issue workstream. In this repo, the roadmap tracks live
as epics:

- `Epic: V1 detector`
- `Epic: V2 explainer and planner`
- `Epic: V3 agentic loop`

An epic should define:

- the concrete workstream outcome
- why it matters now
- the expected child slices
- the sequencing constraints that matter inside the track

Do not let an epic displace a queue-ready child issue.

### Research

Use `Research:` when the real deliverable is a decision-ready recommendation,
not production code.

### Enhancement

Use `Enhancement:` for shipped-surface growth or a new capability.

### Bug

Use `Bug:` for incorrect behavior in shipped functionality, regressions, or
material documentation mismatches.

### Maintenance

Use `Maintenance:` for work that improves the repo, delivery pipeline, or
internal quality without directly expanding user-visible product scope.

Keep `Maintenance:` broad enough to cover both must-do upkeep and lower-priority
developer-experience improvements. Distinguish those slices with issue-form
subtype and project priority rather than adding near-synonymous top-level types.

## Labels

Use a deliberately small label set. Labels should support filtering and
automation, not replace the issue taxonomy.

Preferred labels:

- `enhancement`
- `question`
- `bug`
- `documentation`
- `epic`
- `maintenance`

Rules:

- apply `enhancement` to `Enhancement:` by default
- apply `question` to `Research:` by default
- apply `bug` to `Bug:` by default
- apply `epic` to `Epic:` by default
- apply `maintenance` to `Maintenance:` by default
- do not add phase labels such as `v1`, `v2`, or `v3`
- represent phases as epic issues instead

The checked-in palette manifest lives at `config/labels/label_palette.json`.

## Milestones

Milestones are quarter-focused, not hierarchy-focused.

Milestones answer:

> Which issues are we explicitly targeting in this quarter?

Use milestones like this:

- milestone title is the quarter, such as `2026 Q2`
- due date is the quarter end
- description briefly states the quarter goal
- only assign a milestone to work genuinely targeted for that quarter
- leave non-committed items unmilestoned by default

Do not use milestones for:

- hierarchy
- backlog priority
- generic categorization
- roadmap phase tracking

That separation should stay:

- issue prefix -> work type
- native hierarchy -> structure
- project fields -> workflow and queue order
- milestone -> timing

## Project Model

Use the GitHub Project fields consistently:

- `Status`
  - `Todo`
  - `In Progress`
  - `Done`
- `Priority`
  - `Now`
  - `Next`
  - `Later`
- `Queue Order`
  - numeric ordering within an active priority band

Implementation note:

- `Status` is a GitHub Projects built-in field in the current hosted product
- create or sync `Priority` and `Queue Order`
- reuse the existing built-in `Status` field rather than trying to create a second one

Interpretation:

- queue items:
  - `Now` means queue-ready and worth active attention soon
  - `Next` means likely after current `Now`, or waiting on one meaningful dependency
  - `Later` means valuable but not timely, not ready, or too far behind prerequisite work
- epics:
  - remain in the project for visibility
  - may carry `Priority` so roadmap sequencing stays visible
  - are not queue-ordered work items
  - must not use `Queue Order`

Canonical views:

- `Backlog`: operational queue, excludes epics
- `Epics`: umbrella workstream view, filtered to `epic`

## Issue Content Format

Keep issue bodies concise, decision-oriented, and actionably structured.

Recommended sections for most issues:

- Goal or decision question
- Why now
- Current baseline
- In scope
- Out of scope
- Acceptance criteria or decision output
- Dependencies and blockers
- Validation or evidence expectations

For `Epic:`, emphasize:

- outcome
- boundaries
- sequencing
- child issue expectations

For `Research:`, emphasize:

- decision question
- evidence standard
- recommendation output
- unresolved unknowns that would still block a decision

## Issue Forms

The repo uses typed GitHub issue forms aligned with this taxonomy.

Preferred forms:

- Epic
- Research
- Enhancement
- Bug
- Maintenance

The forms should reinforce the issue content format above instead of creating a
second competing structure.

## Backlog And Agent Work

Repo-shared reusable agent workflows should live under `.agents/skills/`.

Use these guidance surfaces like this:

- `AGENTS.md`: always-on repo-global execution constraints
- `docs/engineering/*`: durable policy and workflow detail
- `.agents/skills/*`: reusable named workflows
- backlog items: deferred or coordinated follow-up work that should not be silently folded into guidance

## Governance Automation

`git-slop` may enforce this policy through manual-first GitHub governance
workflows and deterministic helpers under `agent-tools github ...`.

Automation should prefer:

- read-only audits first
- compact workflow summaries as the primary operator-facing output
- narrow, explicit auto-fixes only where the mutation is low-risk

The approved safe auto-fix set is:

- create the current and next quarter milestones if missing
- update current and next quarter milestone due dates and descriptions
- create or update repo-managed labels from the palette manifest

Automation must not auto-mutate:

- issue titles
- issue bodies
- issue milestone assignments
- project `Priority` or `Queue Order`
- parent/sub-issue links

Project-backed workflows should use the repo secret `GH_PROJECTS_TOKEN`.
