# Runtime and State Troubleshooting

Start with:

```bash
git-slop version
git-slop build-info --format json
git slop doctor --format json
git slop doctor --bundle
```

The JSON error code and pointer identify the failing contract. The diagnostic
bundle is redacted for issue intake; do not attach source, credentials, raw
tokens, absolute paths, or author identities.

## Shallow history

Git Slop fails closed because churn, age, stewardship, and co-change evidence
is incomplete. Fetch full history with `git fetch --unshallow`, or deliberately
use `git slop find --allow-shallow`. Do not compare shallow and complete-history
reports without `compare --force` and a documented reason.

## Adoption and clean worktrees

Reports record staged, modified, and untracked counts plus an analyzed-content
digest. If worktree state changes during analysis, Git Slop aborts before
publishing `latest/`.

If a clean repository is reported dirty, run `git slop init --check`. Repair
missing runtime ignore rules with `git slop init --repair`; this preserves the
repository's existing configuration. Current rules cover reports, cache,
coordination locks, prompt packs, diagnostic bundles, and recovery backups.

## Stale but valid reports

`doctor` reports `current`, `stale`, `unverified`, `invalid`, or `missing`.
Currentness checks HEAD, worktree digest, effective config digest, selected
scope, analyzer version, and report age. Refresh and require a current report:

```bash
git slop find
git slop doctor --require-current
```

Use `health --require-current` or `check --require-current` for local gates.
Without that flag, report consumers warn about stale default reports but can
still render historical evidence.

## Resource limits

Use `git slop doctor`, `resources.memory_budget_mb`,
`organization.max_commit_files`, and `--scope` for large repositories. Files
over `resources.large_file_bytes` retain byte and token context evidence but
skip structural token materialization.

Run `git slop find --estimate-only` before increasing a budget. Estimates show
separate cold- and warm-cache times, state their cache assumptions, and retain a
conservative fixed-overhead memory range. The completed scan receipt records
measured peak memory and elapsed time. Use `--allow-degraded` only when a
deterministic path-prefix report is acceptable.

## Report size and validity

Use `git slop prune --dry-run`, then `git slop prune --yes`. JSON is compact
by default; enable compatibility YAML explicitly with `output.yaml: true`.

Run `git slop find` to replace a missing or invalid report. Consumers validate
the complete schema-5 shape and return stable codes and JSON pointers. Schema-4
input requires `--allow-legacy` or `git slop report migrate`.

## Invalid scopes and selectors

- `invalid_scope`: pass one normalized repo-relative path.
- `scope_not_found`: the path does not exist in the selected repository.
- `empty_scope`: commit the input, or use `--allow-empty-scope` only for an
  intentional empty report.
- `selector_not_found`: list interventions, health findings, relationships, or clusters and copy the
  exact path or identifier.

## Filesystem failures

`io_failure` identifies a read, write, rename, or directory permission problem.
Confirm the repository, `.slop/`, `--state-dir`, and `--output-dir` are
writable. Use `find --ephemeral` to separate adoption permissions from detector
behavior. Repair ownership or choose a writable directory instead of using
`sudo` inside a working copy.

## Configuration migration

Run `git slop config validate`, `config diff-defaults`, and `config schema`.
Unknown keys and unsafe values are rejected. `config migrate` rewrites legacy
configuration as schema 2.

Preview migration before writing and keep the default recovery backup:

```bash
git slop config migrate --dry-run
git slop config migrate
```

Restore `.slop/config.yaml.bak` if an applied migration is not accepted.

## Baseline readiness and drift

`baseline_not_comparison_ready` lists every blocker, message, and report
pointer. Produce a current report from a clean worktree with complete evidence,
then retry. `baseline_drift` means the named baseline has different content;
inspect it and pass `--replace` only for intentional movement. Baseline removal
previews by default; add `--yes` only after confirming the name and state root.
See [Named Baselines](../baselines.md).

## Concurrent scans

`scan_locked` includes the lock path and owner PID when available. Wait for
that process, terminate only a scan you own, or choose another `--state-dir`.
Do not delete a live lock. A stale owner sidecar is removed when the owning lock
is released; the lock file may remain as ignored coordination state.

## Cache problems

```bash
git slop cache status --format json
git slop cache prune --dry-run --format json
```

The default cache is `.slop/cache/`. It is disposable and never required for
correctness. `find --no-cache` disables reads and writes; `find --ephemeral`
also keeps output outside the worktree.

## Generated output collisions

Prompt packs reject file collisions and replace directories atomically only
with `--force`. SARIF, HTML, schemas, manuals, references, diagnostic bundles,
and configuration writes use atomic file replacement. Choose a repository
output or explicitly opt into local-path provenance for an absolute path.
