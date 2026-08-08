# Troubleshooting

## Shallow history

Git Slop fails closed because churn, age, stewardship, and co-change evidence is incomplete. Fetch full history (`git fetch --unshallow`) or deliberately use `git slop find --allow-shallow`. Never compare a shallow report with a complete-history baseline without `compare --force` and a documented reason.

## Dirty or changing worktrees

Reports record staged, modified, and untracked counts plus an analyzed-content digest. A worktree may be dirty, but if its state changes during analysis Git Slop aborts before publishing `latest/`.

## Large repositories or generated assets

Use `git slop doctor`, set `resources.memory_budget_mb`, tune `organization.max_commit_files`, and use `--scope` for a package. Files over `resources.large_file_bytes` retain byte/token context evidence but skip structural token materialization. A resource-bound exit is an environment outcome, not a passing analysis.

## Unexpectedly large reports

Version 0.10 omits structural token arrays and hard-links `latest/` to immutable
runs when the filesystem supports it. Use `git slop prune --dry-run`, then
`git slop prune`. JSON is compact by default. Enable compatibility YAML
explicitly with `output.yaml: true` and prefer JSON for automation.

## Missing or invalid reports

Run `git slop find`. Report consumers validate the complete schema-4 shape and return an invalid-report exit instead of silently substituting zeros.

## Configuration failures

Run `git slop config validate`, `config diff-defaults`, and `config schema`. Unknown keys and unsafe values are rejected. `config migrate` rewrites legacy configuration as schema 2.

## CI permissions and baselines

The Action needs `contents: read`; PR comments additionally need pull-request write permission. Use `baseline-report` for regression-only annotations and keep the absolute repository dashboard in the job summary. A baseline must come from a compatible analyzer/config/history contract.
