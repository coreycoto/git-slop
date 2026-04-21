# Agent Guidance Governance

- Status: current
- Audience: maintainer
- Canonical: yes

This document defines where repo-local agent guidance should live and when it
should become a backlog item instead of another inline note.

## Canonical Surfaces

Use the guidance surfaces like this:

- `AGENTS.md`: always-on, repo-global execution constraints or sharp edges
- `docs/engineering/*`: durable contributor-facing policy and workflow detail
- `.agents/skills/*` and `.agents/skills/_shared/references/*`: reusable named workflows and shared decision aids
- backlog items: deferred or coordinated follow-up work that should not be silently folded into local guidance

## Placement Rules

Put guidance in `AGENTS.md` only when all of the following are true:

- it is repo-global rather than task-local
- it changes how an agent should behave during the current turn
- omitting it would create repeated execution mistakes or false closeouts

Put guidance in `docs/engineering/*` when:

- a contributor should be able to discover it without reading `AGENTS.md`
- the content is durable policy or workflow detail
- the topic benefits from examples, caveats, or command references

Put guidance in a skill or shared reference when:

- the work is a named repeatable workflow
- multiple steps, tools, or deterministic artifacts need to be orchestrated
- the same decision table or workflow guidance will be reused

Create or update a backlog item when:

- the finding should not be fixed in the current change
- follow-up needs explicit prioritization, sequencing, or acceptance criteria
- the current repo state, docs, or skills are materially stale

Backlog creation is not the default outcome. Prefer direct doc, skill, or
shared-reference updates when the change is small and local.
