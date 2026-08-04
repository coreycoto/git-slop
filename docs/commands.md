# Command Guide

Git Slop commands are local-first and deterministic. A typical workflow is:

```text
init -> find -> health/show/explain -> plan -> check
```

`find` runs the detector. The other analysis commands consume existing report
artifacts without rescoring detector truth.

The installed executable is `git-slop`. When it is on `PATH`, Git can also run
it as `git slop`.

## Core Workflow

```bash
git-slop init
git-slop find
git-slop health
git-slop show README.md
git-slop explain --top 5
git-slop plan --path src
git-slop check
git-slop version
git-slop build-info --format json
```

### Init

`init` writes `.slop/config.yaml`, `.slop/.gitignore`, and ensures the generated
state directories exist:

```bash
git-slop init
git-slop init --force
```

Existing generated config files are kept unless `--force` is supplied.

### Find

`find` analyzes tracked files in the current Git worktree:

```bash
git-slop find
```

It writes the same four-file bundle to `.slop/latest/` and to one timestamped
directory under `.slop/runs/`:

- `report.json`
- `report.yaml`
- `summary.md`
- `health.md`

Run from a full-history checkout when history-derived age, churn, coupling, and
stewardship evidence matters. `stats.history_complete` records whether the
repository was shallow.

### Health

`health` projects repository-health evidence from an existing schema-4 report.
It does not rerun the detector:

```bash
git-slop health
git-slop health --report .slop/latest/report.json
git-slop health --format json
git-slop health --format github --max-annotations 10
```

Formats:

- `markdown`: the repository-health dashboard used by `health.md`
- `github`: bounded GitHub workflow-command annotations
- `json`: an automation payload containing the additive health section

The default report is `.slop/latest/report.json`, the default format is
`markdown`, and the default annotation cap is 10. Every format writes to
standard output. `health` never rewrites `.slop/latest/health.md`; only `find`
writes the persisted report bundle. GitHub annotations include a specific next
command such as `git-slop explain --path <path>`.

An abridged Markdown dashboard looks like this:

```markdown
# Repository Health

❌ **Review required** — 1 file(s) and 0 folder(s) exceed configured refactor thresholds.

## Actionable Findings

| Severity | Path | Why it surfaced | Next step |
| --- | --- | --- | --- |
| `error` | `src/parser.rs` | exceeds the configured context budget | `git-slop explain --path src/parser.rs` |
```

The headline is a repository rollup, the finding identifies review evidence,
and `Next step` is a deterministic drill-down command. These findings are
advisory: successful rendering exits 0 even when the dashboard contains
warnings or errors. Use `git-slop check` when findings should enforce policy.

### Show

`show` renders one file or folder record:

```bash
git-slop show README.md
git-slop show src --format json
git-slop show README.md --report path/to/report.json
```

The default format is text-compatible YAML; `--format json` emits JSON.

### Explain

`explain` accepts one file/folder path, relationship ID, cluster ID, or top-N
selection:

```bash
git-slop explain
git-slop explain --path src/report.rs
git-slop explain --path src
git-slop explain --relationship near_duplicate_neighborhood-1234
git-slop explain --cluster concept_cluster-1234
git-slop explain --top 5
git-slop explain --top 5 --format json
```

With no selector, `explain` uses the top five action-queue entries. Its output
keeps stable detector costs separate from overlay context. Findings are
evidence, not correctness proofs or refactor mandates.

### Plan

`plan` requires exactly one path, relationship, or cluster selector:

```bash
git-slop plan --path src
git-slop plan --relationship near_duplicate_neighborhood-1234
git-slop plan --cluster concept_cluster-1234
git-slop plan --path src --max-slices 3
git-slop plan --path src --format json > .slop/latest/plan.json
```

Plan slices include scope paths, out-of-scope paths, supporting evidence,
evidence summaries, and preview-only backlog handoff metadata. The command does
not edit code, invoke a model, mutate GitHub, rerun the detector, or change
scoring.

Use a plan slice as human review guidance: keep edits inside its scope, respect
out-of-scope paths, and review the cited evidence before acting.

### Check

`check` applies the stable file-level threshold gate to an existing report:

```bash
git-slop check
git-slop check --report .slop/latest/report.json
git-slop check --fail-on-context-band warning
git-slop check --fail-on-slop-band high
```

Threshold overrides are evaluated at or above the selected band. Without
overrides, the values come from `.slop/config.yaml`.

Exit codes:

- `0`: no file met either threshold
- `1`: one or more files met a threshold
- `2`: a report, selector, or command input was invalid

Overlay and health evidence do not affect this gate.

### Version

```bash
git-slop version
```

The output has the stable form `git-slop <version>`.

### Build Info

`build-info` prints the machine-readable package and source-build identity used
by release, Action, and Homebrew verification:

```bash
git-slop build-info --format json
```

The schema-1 object contains `project`, `version`, `source_revision`, and
`source_dirty`. Verified release builds contain the full 40-character tag
revision and `source_dirty: false`. Local or source builds that cannot prove Git
identity keep the nullable provenance fields instead of inventing a revision.

## Prompt Packs

`explain` and `plan` can write deterministic prompt packs for optional local
model summarization:

```bash
git-slop explain --top 5 --prompt-pack .slop/prompt-packs/top
git-slop plan --path src --format json \
  --prompt-pack .slop/prompt-packs/src-plan
```

A prompt pack contains:

- `context.json`: selected payload plus minimal report excerpts
- `prompt.md`: local-model instructions
- `README.md`: boundary rules

Prompt packs do not add a model dependency, call a provider, rescore detector
truth, mutate code, or mutate GitHub.

## Advanced Artifact Commands

These commands are read-only projections of existing reports.

### Compare

`compare` consumes two schema-4 reports:

```bash
git-slop compare \
  --base .slop/runs/20260401T120000Z/report.json \
  --head .slop/latest/report.json

git-slop compare \
  --base path/to/base.json \
  --head path/to/head.json \
  --top 20 \
  --format json
```

It reports added, removed, changed, and unchanged file/folder records, stable
score and band movement, overlay pressure deltas, and action-queue movement. It
does not rerun the detector, write `.slop/`, change scoring, or imply causality.

### SARIF

`sarif` exports action-queue findings as SARIF 2.1.0:

```bash
git-slop sarif
git-slop sarif --top 10
git-slop sarif \
  --report .slop/latest/report.json \
  --output .slop/latest/git-slop.sarif
```

Without `--output`, SARIF is written to standard output. The export preserves
stable hotspot cost and overlay evidence as separate properties. It does not
upload results, rerun the detector, change scoring, or mutate GitHub.

## GitHub Action

The repository's composite Action wraps the native release for CI: it verifies
the selected archive, exact tag revision, schema-3 manifest, canonical crate
digest, and installed `build-info`; runs `find` once; appends the persisted
`health.md` to the job summary; renders optional bounded annotations from that
report; and only then applies the optional stable `check` gate. It exposes the
source revision and crate/manifest digests for downstream provenance records.
It can also publish bounded report artifacts or one pull request comment. See
[GitHub Action](github-action.md) for the supported inputs and safe defaults.
