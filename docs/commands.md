# Command Guide

Git Slop commands are local-first and deterministic. A typical workflow is:

```text
doctor/config -> init -> find -> health/show/explain -> plan -> compare/check
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
git-slop doctor --bundle
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

`find` analyzes tracked files in the current Git worktree. It creates the
non-destructive `.slop/.gitignore` when needed:

```bash
git-slop find
git-slop find --scope packages/example
git-slop find --quiet
git-slop find --allow-shallow
```

It writes the same four-file bundle to `.slop/latest/` and to one timestamped
directory under `.slop/runs/`:

- `report.json`
- `report.yaml`
- `summary.md`
- `health.md`

Shallow history fails by default. `--allow-shallow` is an explicit
acknowledgement and the report records incomplete evidence.

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
- `text`: the concise interactive terminal view
- `github`: bounded GitHub workflow-command annotations
- `json`: an automation payload containing the additive health section

The default report is `.slop/latest/report.json`, the default format is
`text`, and the default annotation cap is 10. Every format writes to
standard output. `health` never rewrites `.slop/latest/health.md`; only `find`
writes the persisted report bundle. GitHub annotations include a specific next
command such as `git-slop explain --path <path>`.

The dashboard keeps three related concepts separate:

- **Context/load bands** (`compact`, `healthy`, `warning`, and
  `refactor_required`) describe how much `agent_context` content must be loaded.
  File bands use file tokens; folder bands use direct child-file counts and
  direct tokens.
- **Maintenance-pressure evidence** is the stable `slop_score` and `slop_band`
  derived from deterministic load, history, and coordination signals. It is
  not an overall quality score and is not another name for a context/load
  band.
- **Finding severity** (`notice`, `warning`, or `error`) is the rendered review
  priority. It stays the same in Markdown and GitHub annotations; policy mode
  does not promote or demote it.

Every surfaced warning or refactor-required folder states the exact boundary
that produced its displayed band. For example, `19 direct files > 17 healthy
ceiling` identifies both the observed value and configured boundary. When
direct files and direct tokens both cross the relevant ceiling, both clauses
are shown. The row includes a copyable folder command such as
`git-slop explain --path src/` (`--path .` for the repository root) and one
highest-ranked recursive `agent_context` descendant. That descendant is chosen
deterministically by descending maintenance-pressure score, then descending
tokens, then ascending path.

Markdown number formatting is locale-independent: integer counts and token
totals use comma grouping; non-integral percentiles use comma grouping and two
decimal places; concentration and profile shares use one decimal place plus
`%`; and maintenance-pressure scores use one decimal place. JSON keeps numeric
values and types instead of formatted strings.

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
git-slop show src --format yaml
git-slop show README.md --report path/to/report.json
```

The default is a compact human view; `--format yaml|json` emits the complete record.

### Configuration, diagnostics, discovery, and retention

```bash
git-slop config show --effective
git-slop config validate
git-slop config diff-defaults
git-slop config migrate
git-slop config schema
git-slop doctor --bundle
git-slop list findings --profile data_context --top 20
git-slop list relationships --path src
git-slop prune --dry-run
git-slop completions zsh
git-slop html --output .slop/latest/report.html
```

Global `--repo <path>` avoids changing directories. Diagnostic bundles exclude
source, raw tokens, credentials, absolute paths, and author identities.
The HTML export is self-contained and local-only, with path search, profile and
severity filters, sortable file metrics, and collapsible relationship evidence.

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

For a folder selector, `explain` provides at most five descendant hotspots,
falling back to the highest maintenance-pressure descendants when none are in
the action queue. The health dashboard intentionally previews only the single
highest-ranked `agent_context` descendant, so the folder-scoped command is the
bounded drill-down for the remaining evidence.

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
overrides, values come from the configuration embedded in the immutable report.

Exit codes:

- `0`: command succeeded or no policy threshold was met
- `1`: one or more policy/regression findings met an explicit gate
- `2`: usage, selector, configuration, or report input was invalid
- `3`: Git, filesystem, or repository environment failure
- `4`: analysis was bounded by an explicit resource limit

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
