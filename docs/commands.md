# Command Guide

Git Slop commands are local-first and deterministic. The main workflow is:

```text
init -> find -> show/explain -> plan -> check
```

The detector writes reports; downstream commands consume those reports without
rescoring detector truth.

The installed executable is `git-slop`. When it is on `PATH`, Git can also run
it as `git slop`.

## Core Workflow

```bash
git-slop init
git-slop find
git-slop show README.md
git-slop explain --top 5
git-slop plan --path src/git_slop
git-slop check
git-slop version
```

- `init` writes `.slop/config.yaml` and `.slop/.gitignore`.
- `find` analyzes tracked files and writes `.slop/latest/` plus a timestamped
  `.slop/runs/<timestamp>/` copy.
- `show` renders one file's stable costs and overlay evidence from an existing
  schema-4 report.
- `explain` explains one file, folder, relationship, cluster, or top-N hotspot
  selection from an existing schema-4 report.
- `plan` proposes bounded maintenance slices from an existing schema-4 report.
- `check` evaluates the stable detector gate. It does not use overlays.
- `version` prints the installed CLI version.

## Explain

```bash
git-slop explain --path src/git_slop/reporting.py
git-slop explain --path src/git_slop
git-slop explain --relationship near_duplicate_neighborhood-1234
git-slop explain --cluster concept_cluster-1234
git-slop explain --top 5
git-slop explain --top 5 --format json
```

The output keeps detector cost and overlay context separate. It should help a
maintainer understand why an item is important, which evidence is strongest,
which evidence is adjacent only, and why the finding is not a correctness proof
or refactor mandate.

## Plan

```bash
git-slop plan --path src/git_slop
git-slop plan --relationship near_duplicate_neighborhood-1234
git-slop plan --cluster concept_cluster-1234
git-slop plan --path src/git_slop --max-slices 3
git-slop plan --path src/git_slop --format json > .slop/latest/plan.json
```

Plan slices include scope paths, out-of-scope paths, supporting evidence,
evidence summaries, and preview-only backlog handoff metadata. The command does
not edit code, mutate GitHub, invoke a model, rerun the detector, or change
detector scoring.

### Acting On A Plan

Use a plan slice as human review guidance: keep edits inside its scope paths,
respect out-of-scope paths, and let the cited evidence explain why the work is
bounded. Git Slop does not generate patches, orchestrate refactors, commit,
push, or mutate GitHub.

## Prompt Packs

`explain` and `plan` can write deterministic prompt packs for local model
summarization.

```bash
git-slop explain --top 5 --prompt-pack .slop/prompt-packs/top
git-slop plan --path src/git_slop --format json \
  --prompt-pack .slop/prompt-packs/src-plan
```

A prompt pack contains:

- `context.json`: selected payload plus minimal report excerpts
- `prompt.md`: local-model instructions
- `README.md`: boundary rules

Prompt packs are advisory. They do not add a model dependency, call a provider,
rescore detector truth, mutate code, or mutate GitHub.

## Advanced Artifact Commands

These commands are read-only artifact surfaces. They are useful for automation
and integration work, but they are not part of the core cleanup workflow.

### Compare

`git slop compare` compares two existing schema-4 reports.

```bash
git-slop compare \
  --base .slop/runs/20260401T120000Z/report.json \
  --head .slop/latest/report.json

git-slop compare \
  --base .slop/runs/20260401T120000Z/report.json \
  --head .slop/latest/report.json \
  --format json
```

It reports added, removed, changed, and unchanged file/folder records,
`slop_score` movement, band movement, overlay pressure deltas, and action-queue
movement. It never reruns the detector, writes `.slop/`, changes scoring, or
implies causality.

### SARIF

`git slop sarif` exports action-queue findings from an existing schema-4 report
as SARIF 2.1.0.

```bash
git-slop sarif \
  --report .slop/latest/report.json \
  --output .slop/latest/git-slop.sarif
```

SARIF output preserves stable hotspot cost and overlay evidence as separate
properties. The command does not upload results, rerun the detector, change
scoring, or mutate GitHub.
