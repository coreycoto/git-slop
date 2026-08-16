# Command Guide

Git Slop commands are local-first and deterministic. A typical workflow is:

```text
doctor/config -> find -> health/show/explain -> plan -> compare/check
```

`find` runs the detector. The other analysis commands consume existing report
artifacts without rescoring detector truth. `init` is an optional adoption step
for a team that wants repo-owned configuration and durable `.slop/` state.

The installed executable is `git-slop`. When it is on `PATH`, Git can also run
it as `git slop`.

## Core Workflow

```bash
git-slop find
git-slop health
git-slop show README.md
git-slop explain --top 5
git-slop plan --path src
git-slop check
git-slop init --check
git-slop version
git-slop build-info --format json
git-slop doctor --bundle
```

### Init

`init` writes `.slop/config.yaml`, `.slop/.gitignore`, and ensures the generated
state directories exist:

```bash
git-slop init
git-slop init --check
git-slop init --repair
git-slop init --repair --gitignore-only
git-slop init --force
```

`--check` is read-only and exits 1 when adoption metadata needs repair.
`--repair` adds missing generated ignore rules without replacing repository
configuration. `--gitignore-only` limits check, repair, or force to the ignore
contract, so it never creates or replaces `config.yaml`. Existing generated
config files are kept unless `--force` is supplied; forced replacements are
atomic and keep ignored `.bak` recovery copies. `doctor` prints the exact safe
repair command when it detects adoption drift.

### Find

`find` analyzes tracked files without editing adoption files in the current Git
worktree. Only `init` creates `.slop/config.yaml` and `.slop/.gitignore`:

```bash
git-slop find
git-slop find --scope packages/example
git-slop find --scope intentionally-empty --allow-empty-scope
git-slop find --no-progress
git-slop find --quiet
git-slop find --allow-shallow
git-slop find --state-dir /tmp/git-slop-state --output-dir /tmp/git-slop-output --no-cache
git-slop find --ephemeral
git-slop find --estimate-only
```

Adopted repositories receive the compact bundle in `.slop/latest/` and one
timestamped run. Before adoption, plain `find` writes beneath
`.git/git-slop/ephemeral/`; report commands discover it without `--report`:

- `report.json`
- optional `report.yaml` when `output.yaml: true`
- `summary.md`
- `health.md`

Shallow history fails by default. `--allow-shallow` is an explicit
acknowledgement and the report records incomplete evidence.
`--no-progress` keeps the final report summary but disables phase updates;
`--quiet` suppresses both.
Automatic first-run storage reuses compatible Git-private cache entries.
Explicit `--ephemeral` disables caching. Scan receipts report disjoint analyzed,
ignored, missing, binary, and undecodable counts alongside timing and resources.

### Health

`health` projects repository-health evidence from an existing schema-5 report.
It does not rerun the detector:

```bash
git-slop health
git-slop health --report .slop/latest/report.json
git-slop health --format json
git-slop health --format github --max-annotations 10
git-slop health --require-current
```

The default is the latest durable or Git-private report. Text is the default
format and annotations default to 10. Findings are severity-first and state
shown/total counts. Every format writes to standard output. `health` never
rewrites `.slop/latest/health.md`; only `find` persists the bundle.

See [Health Output](health-output.md) for format contracts, band semantics,
folder boundary explanations, deterministic descendants, and number rendering.

An abridged Markdown dashboard looks like this:

```markdown
# Repository Health

❌ **Review required** — 1 actionable file(s) exceed configured context budgets; 0 derived/classified file(s) and 0 folder(s) remain investigation context.

## Advisory Health Findings

Showing 1 of 1 advisory finding(s), ordered by review severity and then
maintenance pressure.

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
git-slop config migrate --dry-run
git-slop config schema
git-slop doctor --bundle
git-slop doctor --scope packages/example --format json
git-slop doctor --require-current
git-slop list policy-failures --top 20
git-slop list interventions --profile data_context --top 20
git-slop list observations --top 20
git-slop list health-findings --severity warning --top 20
git-slop list relationships --path src
git-slop prune --dry-run --format json
git-slop prune --yes
git-slop cache status --format json
git-slop cache prune --dry-run --format json
git-slop completions zsh
git-slop html --output .slop/latest/report.html
```

Global `--repo <path>` avoids changing directories. Diagnostic bundles exclude
source, raw tokens, credentials, absolute paths, and author identities.
The self-contained HTML export separates the four decision surfaces and offers
independent filters, direct paging, and visible master-detail evidence.
Run pruning is preview-only unless `--yes` is supplied; `--dry-run` remains an
explicit, script-friendly spelling of the default.

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

The schema-2 object contains `project`, `version`, `source_revision`,
`source_dirty`, target, crate identity, and build-source fields. Verified
release builds contain the full 40-character tag revision and
`source_dirty: false`. Local or source builds that cannot prove Git identity
keep nullable provenance fields instead of inventing a revision.

### Named baselines

```bash
git-slop baseline ensure --name main
git-slop baseline inspect --name main
git-slop compare --baseline main --fail-on-regression
git-slop baseline remove --name main          # preview
git-slop baseline remove --name main --yes    # apply
```

See [Named Baselines](baselines.md) for readiness, storage, drift, update, and
recovery semantics.

## Prompt Packs

`explain` and `plan` can write deterministic prompt packs for optional local
model summarization:

```bash
git-slop explain --top 5 --prompt-pack .slop/prompt-packs/top
git-slop plan --path src --format json \
  --prompt-pack .slop/prompt-packs/src-plan

# Explicitly add bounded local repository context (256-4096 bytes per file).
git-slop explain --top 5 --prompt-pack .slop/prompt-packs/contextual \
  --include-repository-context --excerpt-bytes 2048
```

A prompt pack contains:

- `context.json`: selected payload, explicit report provenance and truncation,
  plus minimal report excerpts
- `prompt.md`: local-model instructions
- `README.md`: boundary rules
- `manifest.json`: SHA-256 bindings and source-report provenance

Repository content is excluded by default. `--include-repository-context`
opts into at most ten selected source/test excerpts, root guidance files, and
inferred verification commands. Every read is repo-relative, rejects symlinks
and traversal, and applies the configured per-file byte limit.

Prompt packs do not add a model dependency, call a provider, rescore detector
truth, mutate code, or mutate GitHub.

## Advanced Artifact Commands

These commands are read-only projections of existing reports.

### Compare

`compare` consumes two schema-5 reports (or explicitly migrated legacy input):

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

`--fail-on-regression` uses the same policy as the Action: a new file regresses
only when it is a finding; an existing file regresses on a worse band or a
configured material score increase with changed content. `--force` records
exact compatibility mismatches in JSON.

```bash
git-slop report validate .slop/latest/report.json
git-slop report schema
```

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
