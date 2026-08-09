# `git-slop health` Reference

## Command Contract

`health` projects repository-health evidence from an existing schema-5 report.
It does not run the detector, rescore findings, or modify report artifacts. The
default input is `.slop/latest/report.json`; use `--report` to select another
report.

```bash
git-slop health
git-slop health --report path/to/report.json
git-slop health --format json
git-slop health --report path/to/report.json --format json
git-slop health --format github --max-annotations 10
```

Every format writes to stdout. A prior `git-slop find` writes
`.slop/latest/health.md`; a later `git-slop health` renders from the report but
does not rewrite that file. Shell redirection can persist a rendering when the
caller explicitly wants a separate output file.

## Formats

| Format | Output | Use |
| --- | --- | --- |
| `markdown` | The human repository-health dashboard used by `health.md` | Terminal review or redirected job-summary content |
| `json` | The additive `health` object | Automation that needs bands, distributions, watchlists, and findings |
| `github` | Escaped GitHub workflow-command annotations | Bounded CI annotations with each finding's `next_command` |

`markdown` is the default. For `github`, `--max-annotations` defaults to 10;
set it explicitly when the caller needs a different bound.

## Exit And Enforcement Semantics

- Exit `0` means the selected report rendered successfully, even when it has
  health findings.
- Exit `2` means the report is missing or incompatible, or the command input is
  invalid.
- Exit `1` is an unexpected execution or report-read failure.

Do not use `health` as the required gate. Use `git-slop check` for the stable
file-level threshold contract: exit `0` passes, exit `1` reports threshold
findings, and exit `2` reports invalid input. Health rollups and overlay
evidence do not change that gate. `check` reads the report and repository
config, writes its result to stdout, and does not modify report artifacts.
Configure the gate with `check.fail_on_context_band` and
`check.fail_on_slop_band`, or override those values with the corresponding
`--fail-on-*` options.

## GitHub Action Mapping

The Action deliberately separates analysis, presentation, and enforcement:

1. Install and checksum-verify the selected native release.
2. Resolve the worktree root, reject shallow history, and run `git-slop find`
   exactly once.
3. Append the generated `.slop/latest/health.md` to `GITHUB_STEP_SUMMARY`.
4. When annotations are enabled, run `git-slop health --report
   .slop/latest/report.json --format github --max-annotations <n>`.
5. Upload an allowlisted artifact set and, when enabled, update one pull request
   comment from `health.md`.
6. Publish advisory status, or run `git-slop check --report
   .slop/latest/report.json` after publication when `policy: enforce`.

Thus `find` creates the durable report and Markdown, `health --format github`
projects annotations from that report, and `check` alone supplies the optional
threshold failure. Advisory findings do not fail the Action; installation,
checkout-depth, detector, renderer, or publication failures still do.

## Interpretation

- Treat file and folder bands, distributions, watchlists, and concentration as
  rollups of existing detector facts, not a second score.
- Keep stable hotspot costs separate from organization, verification,
  navigation, blast-radius, stewardship, and semantic-drift overlays.
- Follow a finding's deterministic `next_command`, usually a targeted
  `git-slop explain`, before proposing maintenance.
- Treat `budget_exceeded` as a review threshold, not proof that a refactor is
  correct or mandatory.
