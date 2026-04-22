# Git Slop

Find the files that cost too much context.

Git Slop is a local-first detector for AI-era repositories. It scans a Git
repo, measures token cost, age, and churn, then ranks the files and folders
most worth refactoring. It also emits an experimental organization-health
overlay for coordination-cost evidence such as duplication, temporal coupling,
and boundary leakage.

The maintainer-only backlog governance surface now lives under `agent-tools`
and `.agents/`. That internal tooling manages GitHub issue forms, quarter
milestone policy, label palette drift checks, and repo-local skill metadata.

## Why Git Slop Exists

Traditional static analysis tells you whether code violates rules.

Git Slop answers a different question:

> Which files are expensive to load, reason about, retrieve, and safely change?

And now, in a separate experimental layer:

> Which concepts are expensive to coordinate because they are duplicated,
> scattered, or forced to co-change across boundaries?

That matters for humans. It matters even more for LLM-assisted development.

## What V1 Does

- scan tracked text files in a Git repository
- ignore generated dependency lockfiles by default
- count token cost
- measure file age from Git history
- measure churn from Git history
- rank hotspots
- emit JSON, YAML, Markdown, and terminal output
- support CI checks
- create a machine-readable action queue for humans and agents
- emit experimental organization-health evidence:
  - `organization_metrics`
  - `relationships`
  - `clusters`

## What V1 Does Not Do

- automatically rewrite code
- require hosted APIs
- send repo data anywhere
- use an LLM for scoring
- fold organization-health pressures into `priority_score`

## Quickstart

```bash
uv run git-slop init
uv run git-slop find
uv run git-slop show README.md
uv run git-slop check
uv run git-slop version
uv run git-slop --help
```

Planned install methods after the detector is real:

- `uv tool install git-slop`
- `pipx install git-slop`
- Homebrew tap support later

## Command Surface

- `git slop init`
- `git slop find`
- `git slop show`
- `git slop check`
- `git slop version`

The package exposes both:

- `git-slop ...`
- `python -m git_slop ...`

## Planned Generated State

Git Slop will write generated artifacts under `.slop/`:

```text
.slop/
  config.yaml
  .gitignore
  latest/
  runs/
  cache/
```

`report.json` is the machine contract. `summary.md` is the human summary.
The report timestamp reflects the analyzed repo snapshot so repeated runs on the
same HEAD can stay byte-identical.

The main hotspot queue stays driven by context cost:

- token size
- age
- churn

The organization-health layer is parallel evidence only. It does not currently
change `priority_score`, `priority_band`, or `git slop check`.

Generated dependency lockfiles such as `uv.lock`, `package-lock.json`, and
`poetry.lock` are ignored by default so hotspot rankings stay focused on
refactor targets rather than generated manifests.

For mature repos with major file moves, set `history.follow_renames: true` in
`.slop/config.yaml`. That is slower, but it preserves age and churn signals
across renames instead of treating moved files as brand new.

`find` writes:

- `.slop/latest/report.json`
- `.slop/latest/report.yaml`
- `.slop/latest/summary.md`
- `.slop/runs/<timestamp>/...`

`report.json` always includes these experimental namespaces:

- `organization_metrics`
- `relationships`
- `clusters`

## Project Docs

- [Vision](docs/vision.md)
- [Architecture](docs/architecture.md)
- [Scoring Model](docs/scoring-model.md)
- [Roadmap](docs/roadmap.md)
- [Backlog Governance](docs/engineering/backlog-governance.md)

## Maintainer Tooling

The public CLI stays focused on detector behavior. Backlog governance and
repo-local agent tooling live under:

- `agent-tools ...`
- `uv run agent-tools skills sync-openai-metadata --repo-root . --check`
- `.agents/skills/...`
- `config/github/project_config.json`
- `config/labels/label_palette.json`
- `config/agents/skill_metadata_manifest.json`
- [Agent Tools Extraction](docs/engineering/agent-tools-extraction.md)

While `coreycoto/agent-tools` remains private, GitHub Actions needs either
`AGENT_TOOLS_READ_TOKEN` or `GH_PROJECTS_TOKEN` with read access to that repo
before `uv sync --group dev` can install the tagged dependency. If
`agent-tools` becomes public later, that extra token is no longer required.

## Philosophy

Readable code can still be slop.

If a file costs too much context, stays large for too long, and changes too
often, it is expensive even when it looks clean.

And a repo can also be slop when an idea leaks across too many medium-sized
files. Git Slop treats that coordination cost as a separate layer so the
detector stays explainable instead of collapsing everything into one opaque
number.

Git Slop exists to make that cost visible.
