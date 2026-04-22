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
- `context_band`
- `priority_score`
- `priority_band`
- `reason_codes`
- `schema_version`

`report.json` is the machine-facing source of truth. `summary.md` is the
human-facing surface.

## CLI Surface

The current command surface is:

- `git slop init`
- `git slop find`
- `git slop show`
- `git slop check`
- `git slop version`

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
