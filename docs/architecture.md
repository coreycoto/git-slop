# Architecture

## Design Goals

The architecture should stay small and boring.

That is a feature. The detector should be easy to run locally, easy to inspect,
and easy to extend without turning into a framework.

## Runtime Pipeline

```text
Git repository
  -> tracked-file inventory via git ls-files
  -> safe text reader with binary/decode filtering
  -> token counting
  -> line and byte metrics
  -> Git history mining
  -> scoring engine
  -> organization-health analyzers
  -> reports and action queue
  -> CLI output, CI summaries, and agent-readable JSON
```

## Inventory Layer

Inventory uses Git as the source of truth:

- `git ls-files -z` provides the tracked-file set
- generated dependency lockfiles are ignored by default
- missing files are skipped safely
- obvious binaries are skipped
- undecodable text files are skipped
- directory walking does not define scope

This keeps the detector deterministic and aligned with what the repo actually
tracks.

## Metrics Layers

### Context Metrics

For each tracked text file, Git Slop computes:

- bytes
- lines
- token count
- `context_band`

### History Metrics

For each tracked text file, Git Slop computes:

- first-seen age
- revisions within the trailing window
- additions and deletions
- relative churn

Rename following stays opt-in because it is slower and more expensive.
When enabled, Git Slop switches to per-file history walks so renamed files keep
their lineage instead of resetting age and churn to the newest path only.

### Organization-Health Metrics

After v1 scoring, Git Slop now runs a second always-on analysis stage that
keeps the main detector score intact while adding coordination-cost evidence.

The current experimental analyzers emit:

- duplicate and near-duplicate token neighborhoods
- commit-level diffusion records
- temporal coupling edges
- lexical affinity edges
- cross-boundary leakage edges
- structural clusters and consolidation candidates

That layer is deterministic, repo-local, and mechanical. It does not use AST
parsers, hosted services, or LLM-based judgment.

## Scoring Layer

The scoring engine combines three pressures:

- `context_pressure`
- `age_pressure`
- `churn_pressure`

These produce:

- `priority_score`
- `priority_band`
- `reason_codes`

The architecture deliberately keeps raw context cost separate from refactor
urgency. A large new file may be context-expensive without yet being the top
refactor candidate. A large, old, high-churn file is the real hotspot.

The organization-health layer remains separate again. Duplication, diffusion,
coupling, and boundary leakage are evidence for coordination cost, not a hidden
fourth weight inside `priority_score`.

## Output Surfaces

Stable outputs belong under `.slop/`:

```text
.slop/
  config.yaml
  .gitignore
  latest/
    report.json
    report.yaml
    summary.md
  runs/
  cache/
```

Planned report contract fields:

- repo metadata
- config metadata
- `files`
- `folders`
- `action_queue`
- `organization_metrics`
- `relationships`
- `clusters`
- `context_band`
- `priority_score`
- `priority_band`
- `reason_codes`
- `schema_version`

`report.json` is the machine-facing source of truth. `summary.md` is the
human-facing surface.

`organization_metrics`, `relationships`, and `clusters` are always emitted and
explicitly marked experimental. `git slop check` ignores them entirely for now.
The report timestamp tracks the analyzed source snapshot so cold and warm runs
on the same HEAD can remain byte-identical.

## CLI Surface

The current command surface is:

- `git slop init`
- `git slop find`
- `git slop show`
- `git slop check`
- `git slop version`

`git slop show` now appends organization-health overlay data, strongest
relationships, and cluster memberships for the selected file or folder.

The package is published as `git-slop` so Git can expose `git slop ...` via its
external command discovery behavior.

## CI and Dogfooding

The maintained CI flow is:

1. install the package and dev tooling
2. verify generated skill metadata through standalone `agent-tools`
3. run unit and integration coverage

The dogfood workflow is separate and does this:

1. check out full history
2. run Git Slop on Git Slop
3. upload report artifacts
4. publish `summary.md` into the Actions job summary
5. enforce thresholds after artifact publication in warn-first mode

The “publish first, fail second” rule matters so failures still leave usable
artifacts behind for inspection.
