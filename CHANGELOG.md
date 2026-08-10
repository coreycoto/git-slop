# Changelog

## 0.11.1 - 2026-08-09

This patch carries the complete 0.11 release onto a source tag whose Action
installer and distribution assets share the same verified contract. The
`v0.11.0` crate and signed tag remain immutable, but its GitHub Release and
Marketplace Action were not published after exact-tag smoke testing exposed
the incompatibility.

### Release correctness

- The Action installer requires the complete 12-asset release, independently
  bounds both SBOM formats, and authenticates their GitHub digests through
  `SHA256SUMS`.
- Native archive verification now requires the five canonical shell completion
  files and one or more safely named, versioned JSON schemas while continuing
  to reject missing, duplicate, unexpected, or unsafe members.
- Release recovery imports the signing key only from the protected release
  environment, validates the configured full fingerprint, and derives the tag
  email from the selected key UID instead of hardcoding an identity.
- Recovery cleanly distinguishes immutable candidate source from current
  control tooling, reconciles an existing draft without retagging, and runs the
  final seven-platform smoke matrix from the exact release tag.

There are no CLI, configuration, or report-schema changes from 0.11.0.

## 0.11.0 - 2026-08-09

This release completes the post-0.10 adversarial audit across detector trust,
comparison semantics, schema contracts, scale, human/agent UX, and release
provenance.

### Correctness and trust

- `find` is read-only with respect to adoption files; scan locking moved under
  `.git/git-slop/`, and ephemeral `--state-dir`, `--output-dir`, and
  `--no-cache` scans are first-class.
- Comparisons use stable repository and normalized scope-selector identity,
  allow ordinary file additions/removals, separate content, metric, and
  evidence status, and distinguish source regressions from evidence-only drift.
- Reports publish analysis-contract and split configuration digests. Local
  remotes are irreversibly redacted and no-remote repositories use root-commit
  identity.
- Strict schema 5 replaces implicit legacy acceptance. Validation reports all
  violations with stable codes and JSON pointers; schema 4 requires
  `--allow-legacy` or explicit migration.
- Typed error classes, JSON errors, explicit-report operation outside a Git
  checkout, corrected history caps, unborn/empty evidence states, and safe
  broken-symlink handling close the remaining edge contracts.
- The Action preserves successful head artifacts on comparison errors, splits
  health/policy/regression/annotation counts, reports a five-state baseline
  status, uses report generation time for freshness, supports safe revision
  ancestry operators, and records ancestry, divergence, and copied-config
  baseline materialization.

### Report, detector, and scale

- Normalized schema 5 stores relationship evidence once, separates exhaustive
  `ranked_files` from actionable `action_queue`, offers compact/standard/full
  evidence profiles, and supports gzip/zstd report artifacts.
- Compare supports bounded detail, pagination, and NDJSON. `git slop schema`
  publishes every machine contract, and `compare --base-ref` reproduces the
  isolated Action baseline workflow locally.
- Co-change evidence now downweights broad changes, treats merge/import/release
  commits explicitly, uses uncertainty-aware confidence, caps final incident
  relationships, reranks retained IDs, and reports suppression diagnostics.
- Verification classification, concept-dispersion naming/extraction,
  low-variance saturation, stewardship support, measured churn, language-aware
  structural extraction, and streamed history improve evidence fidelity.
- Preflight estimates now model history breadth and symlinks; runtime RSS
  checkpoints enforce real budgets. Large files are bounded early, token data
  uses an incrementally limited SQLite LRU, and run/cache retention supports
  byte limits plus machine-readable status and dry-run pruning.

### Experience and distribution

- Explain and plan render relationship endpoints, provenance, evidence state,
  and executable provider-neutral verification. Prompt packs carry complete
  digests/times/truncation provenance and optional bounded context.
- Terminal tables have truthful headers and predictable width controls. The
  HTML explorer uses a compact payload, lazy evidence views, detail panels,
  deep-linked filters, keyboard-sortable tables, and action/health views.
- Generated schemas, man/reference output, completions, signed annotated tags,
  SBOMs, and provenance-rich native archives strengthen distribution. Homebrew
  installs shell completions from the live command tree; archives include Bash,
  Zsh, Fish, PowerShell, and Nushell sources.

Migration notes:

- Migrate stored schema-4 baselines with `git slop report migrate`, or use
  `--allow-legacy` only for a deliberate compatibility read.
- Use `ranked_files` for exhaustive ranking and `action_queue` for attention
  records. The report term is `concept_dispersion`; the compatibility config
  namespace remains `semantic_drift`.
- Health output uses `budget_exceeded` instead of the former
  `refactor_required`/`critical` context aliases. The legacy config key remains
  accepted during this config-schema cycle.

## 0.10.1 - 2026-08-08

This patch release hardens the 0.10 report and automation contracts after a
the audit available at publication time. Highlights include broad canonical validation,
normalized scope identity, byte-level drift detection, one native regression
policy shared by the CLI and Action, strict config bounds, compact JSON and
opt-in YAML, uncapped canonical findings, scan locking, cache retention,
Unicode/BOM-aware inventory, normalized co-change evidence, and neutral archive
ownership.

Schema-4 validation in this release was not exhaustive; schema 5 in 0.11.0
supersedes that contract with strict runtime and published-schema validation.

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
