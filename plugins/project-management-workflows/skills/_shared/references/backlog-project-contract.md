# Backlog Project Contract

Each consumer repository should define one canonical backlog project and treat
its native project fields and issue links as the live source of truth.

Default rules:

- use the repository's checked-in project config as the machine-readable contract
- prefer native GitHub fields over issue-body prose
- use milestones for time-bound commitments rather than roadmap hierarchy
- keep epics in the project but out of queue-order fields unless the repo says otherwise

## Project Fields

If the repository uses queue-centric planning, prefer this field model:

- `Status`
  - `Todo`
  - `In Progress`
  - `Done`
- `Priority`
  - `Now`
  - `Next`
  - `Later`
- `Queue Order`
  - numeric ordering within the active priority band

Interpretation defaults:

- queue items may use `Priority` and `Queue Order`
- epics may stay visible in the project and carry `Priority`
- epics should not consume queue order unless the repo explicitly says otherwise

## Milestones

Use milestones for explicit time-bound commitments, not for hierarchy.

Default rules:

- milestone title should describe the time window, such as a quarter
- due date should match the end of that commitment window
- description should stay brief and outcome-oriented
- leave non-committed work unmilestoned by default

Do not use milestones for:

- roadmap phase naming
- generic categorization
- backlog priority

## Issue Content

Keep issue bodies concise and decision-oriented.

Recommended sections for most issues:

- Goal or decision question
- Why now
- Current baseline
- In scope
- Out of scope
- Acceptance criteria or decision output
- Dependencies and blockers
- Validation or evidence expectations
