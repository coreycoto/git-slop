# Named Baselines

Named baselines turn a reviewed report into a stable local comparison point.
They live under Git-private storage by default, so they do not dirty the
worktree or require a committed report fixture.

## Create a ready baseline

Start from a current, complete report produced by a clean worktree:

```bash
git slop find
git slop doctor --require-current
git slop baseline ensure --name main
git slop baseline inspect --name main
```

`ensure` is idempotent. It creates `main` once, succeeds unchanged when the
same report is supplied again, and fails closed if a different report would
replace it. Apply an intentional replacement explicitly:

```bash
git slop baseline ensure --name main --replace
```

A baseline is comparison-ready only when analysis and inventory are complete,
required evidence is present, and the source report records a clean worktree.
Human errors list every blocker with its stable code and JSON pointer. Use
`--allow-dirty` or `--allow-incomplete-evidence` only when the resulting loss
is understood and recorded; neither flag repairs missing evidence.

## Compare and ratchet

After making one reviewed change:

```bash
git slop find
git slop compare \
  --baseline main \
  --head .slop/latest/report.json \
  --detail summary \
  --fail-on-regression
```

Exit `0` means no native regression was found. Exit `1` means an existing file
worsened or a newly added file entered the finding set. Exit `2` means the
reports, identity, evidence, or command input were incompatible.

`git slop plan` emits the same `baseline ensure`, rescan, and compare commands.
When its source report is inside the repository, those commands use a
copy-pasteable repo-relative path without exposing an absolute local path.

## Inspect, validate, update, and remove

```bash
git slop baseline list
git slop baseline inspect --name main --format json
git slop baseline validate --name main
git slop baseline update --name main
git slop baseline remove --name main          # preview only
git slop baseline remove --name main --yes    # apply
```

`update` requires an existing name and a ready report. `remove` is preview-only
unless `--yes` is present. Baseline objects are content-addressed and compressed;
references and objects stay together under the selected baseline root.

## Storage and portability

The default root is Git's private `git-slop/baselines` directory. Override it
for a disposable job or shared local cache without changing the repository:

```bash
git slop baseline --state-dir "$RUNNER_TEMP/git-slop" ensure --name pr-base
git slop compare --state-dir "$RUNNER_TEMP/git-slop" \
  --baseline pr-base --fail-on-regression
```

Named baselines are machine-local state, not release artifacts. For GitHub
Actions, prefer `baseline-ref` when the base revision is available or
`baseline-report` when a reviewed report is intentionally stored elsewhere.

## Recovery

- `baseline_not_comparison_ready`: resolve each listed blocker, rerun `find`,
  and retry.
- `baseline_drift`: inspect both digests; pass `--replace` only when movement is
  intentional.
- `baseline_not_found`: check `baseline list`, spelling, and `--state-dir`.
- compatibility failure during `compare`: regenerate both sides with the same
  analyzer, tokenizer, scope, effective configuration, and history completeness.
