# Roadmap

## Current State

Git Slop now has:

- a real detector CLI
- repo-local governance docs and issue forms
- a standalone `agent-tools` maintainer dependency
- JSON, YAML, Markdown, and terminal reporting
- a seeded GitHub Project and issue graph

The next work is detector hardening, dogfooding, and then the `v2` explainer
and planner surface.

That detector-hardening phase now includes an organization-health overlay that
stays inside the detector track rather than being folded into `v2` narration.

## Epic Tracks

Roadmap phases are represented as epic issues, not as native GitHub milestones.
Native milestones are reserved for quarter commitments only.

### `Epic: V1 detector`

- bootstrap repo and CLI
- tracked-file inventory
- token counts
- history miner
- scoring
- reports
- `init`
- `check`
- tests
- dogfood workflow
- organization-health evidence:
  - duplicate neighborhoods
  - near-duplicate neighborhoods
  - temporal coupling
  - diffusion and boundary clusters

### `Epic: V2 explainer and planner`

- `git slop explain`
- `git slop plan`
- optional Ollama backend
- theme and seam detection
- richer agent skills and hooks

### `Epic: V3 agentic loop`

- bounded refactor loop
- SARIF
- trend comparisons
- editor integrations
- hosted exploration later, if warranted

## Week-One Bar

By the end of the first week, Git Slop should have:

- the repo scaffolded
- docs committed
- quarter milestones and epic issues opened
- `git-slop` runnable with `uv`
- `git slop init` working
- `git slop find` producing a first report
- CI green
- a dogfood workflow running

That is enough to show the project publicly.

## Issue Seed Order

Open `Epic: V1 detector` leaf issues in this order:

1. bootstrap package, docs, CLI skeleton, and issue templates
2. `init`
3. tracked-file inventory
4. token counting
5. Git history miner
6. scoring engine and reason codes
7. JSON, YAML, Markdown, and terminal reports
8. `check`
9. tests and fixture repos
10. dogfood CI

Seed `v2` issues for:

- `git slop explain`
- `git slop plan`
- optional Ollama backend
- theme and seam detection
- richer agent hooks

Seed `v3` issues for:

- bounded refactor loop
- SARIF
- trend comparisons
- editor integrations

## Explicit Not Yet

Keep these out of milestone-critical work for now:

- website polish
- newsletter or blog platform work
- hosted SaaS
- autonomous refactoring
- editor plugins
- Homebrew tap work
- investor story refinement
- LLM-backed scoring
- folding organization-health into `priority_score`

## Delivery Sequence

Recommended delivery train:

1. `chore: bootstrap git-slop package, docs, and issue templates`
2. `feat: add tracked-file inventory and token-count detector`
3. `feat: add git history mining for file age and churn`
4. `feat: add scoring engine, action queue, and report writers`
5. `test: add fixture repos, parser coverage, and report snapshots`
6. `ci: add dogfood workflow with artifact and summary publishing`
7. `feat: add organization-health evidence without changing hotspot scoring`

That sequence is now effectively the `v1-detector` hardening checklist.
