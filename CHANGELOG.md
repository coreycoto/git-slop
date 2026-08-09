# Changelog

## 0.10.1 - 2026-08-08

This patch release hardens the 0.10 report and automation contracts after a
full adversarial audit. Highlights include exhaustive canonical validation,
normalized scope identity, byte-level drift detection, one native regression
policy shared by the CLI and Action, strict config bounds, compact JSON and
opt-in YAML, uncapped canonical findings, scan locking, cache retention,
Unicode/BOM-aware inventory, normalized co-change evidence, and neutral archive
ownership.

Migration notes:

- `health --format json` now returns a versioned command envelope with the
  health payload under `health`.
- `check --format json` is concise by default; pass `--details` for full records.
- `list --format json` returns a versioned envelope and truncation metadata.
- `report.yaml` is generated only when `output.yaml: true`; `report.json` is
  compact unless `output.pretty_json: true`.
- Schema-2 effective config no longer emits schema-1 aliases.
- Release archives place the man page at `man/git-slop.1`.

## 0.10.0 - 2026-08-08

This release hardens Git Slop's deterministic evidence and makes established-repository adoption incremental.

### Breaking and migration notes

- Configuration is now strict. Unknown keys, wrong types, invalid ranges, non-monotonic bands, and scoring weights that do not sum to `1.0` fail closed. Run `git slop config validate` before upgrading and `git slop config migrate` for schema-1 files.
- Shallow history now fails by default. Use a full clone or explicitly pass `find --allow-shallow`; incomplete history is recorded in the report.
- `health` defaults to a concise terminal view. Use `health --format markdown` for the prior dashboard output.
- `show` defaults to a compact human view. Use `--format yaml` or `--format json` for the complete record.
- `check` uses thresholds embedded in the immutable report unless command-line overrides are supplied.
- `compare` rejects incompatible repository, tokenizer, analyzer, configuration, or history evidence unless `--force` is explicit.

### Trust and scale

- Added worktree cleanliness, staged/modified/untracked counts, sanitized remote identity, analyzed-content digest, mid-scan drift rejection, evidence-completeness metadata, and complete report-shape validation.
- Wired every formerly inert public option and added configurable source/test mappings plus inline test detection.
- Added distinctive-vocabulary filtering, overlay saturation suppression, and separate ownership-concentration, many-author coordination, and stale-ownership signals.
- Bounded Git history, bulk commits, neighbor maps, temporal edges, and preflight memory; large assets keep context cost without retaining structural-token arrays.
- Added content-addressed token/structural caches with hit/miss diagnostics and exact JSON/YAML report-size diagnostics.
- Hard-linked immutable run artifacts into `latest/` where supported and added automatic retention plus `git slop prune`.

### CLI and automation

- Added conventional `--version`/`-V`, global `--repo`, scoped scans, config inspection/migration, `doctor --bundle`, list/filter commands, generated completions, a self-contained HTML explorer, concise health/show output, JSON/GitHub check output, comparison ratchets, and stable exit categories.
- Added Action baseline reports, regression-only PR annotations, explicit scoped scans, and shallow-history acknowledgement.
- Improved SARIF rule identities, help links, fingerprints, analyzer metadata, and baseline state.
- Expanded release archives to macOS Intel and static Linux musl, protected the declared Rust 1.85 MSRV in CI, and added a changed-files contributor validation command.

See [Troubleshooting](docs/troubleshooting.md), [Configuration Recipes](docs/config-recipes.md), and the [Worked Example](docs/worked-example.md).
