# Changelog

## 0.14.0 - 2026-08-14

This release completes the post-0.13.0 user and developer experience follow-up
with safer first-run behavior, clearer evidence, an accessible portable report,
and release automation that cleans up its exact downstream state.

### Safer adoption and current evidence

- Keep unadopted scans Git-private and ephemeral by default, require an explicit
  persistence opt-in, and give `doctor` and repository failures concise recovery
  guidance, including the empty-repository case.
- Add `--require-current` to every report-consuming command and baseline
  operation so stale, invalid, missing, or unverified evidence can fail closed
  before it drives maintenance work.
- Make cache pruning preview-only until explicitly confirmed and emit complete,
  copy-pasteable invocations in generated references.

### Clearer analysis and reporting

- Present conservative estimates in human and machine formats, add
  finding-specific list help and tables, rank globally before applying `--top`,
  and collapse repeated evidence into explicit clusters.
- Separate low-support relationships from incomplete analysis and make empty
  explanations concise without blank provenance sections.
- Give each HTML record a stable identity and deep link, use view-specific sort
  defaults, expose curated reasons and commands, and keep tables, controls, and
  keyboard interaction accessible from narrow mobile layouts through desktop.

### Installation, release, and contributor workflows

- Install into created destinations, activate bundled completions, and provide
  executable release-manifest, checksum, size, attestation, and installed-build
  verification for Unix and Windows archives.
- Align Action alias lifecycle guidance, keep the seven-target release matrix in
  one validated source of truth, gate marketplace instructions on actual client
  capability, and move the completed 0.9.4 publishing migration into history.
- Delete only the consumed exact-version Scoop automation branch after qualified
  bucket `main`, and make changed-file validation retain staged, unstaged, and
  untracked work even when `HEAD` and the comparison base have equal trees.

## 0.13.0 - 2026-08-14

This release removes the remaining first-run, report-currentness, and
maintenance-workflow friction found in the complete 0.12.1 user and developer
experience audit.

### Safe adoption and current evidence

- Keep scan locks, owner sidecars, reports, caches, prompt packs, diagnostics,
  and recovery backups Git-private so a clean adopted repository remains clean
  and comparison-ready.
- Add read-only adoption checks, selective repair, ignore-only repair, and
  Git-private ephemeral scans without replacing repository configuration.
- Distinguish current, stale, unverified, invalid, and missing reports using
  revision, worktree, analyzer, configuration, scope, and age evidence; expose
  explicit currentness gates for local and CI consumers.
- Make configuration and generated-output writes atomic, retain recovery
  backups, and require preview-then-confirm behavior for pruning and baseline
  removal.

### Clearer CLI and maintenance workflows

- Print complete scan receipts, skipped-path explanations, conservative
  resource estimates, classification-aware health summaries, and
  copy-pasteable repo-relative plan and baseline commands.
- Keep source findings actionable while presenting generated, fixture,
  vendored, documentation, and data evidence as investigation context.
- Expand baseline errors, lifecycle guidance, troubleshooting, worked examples,
  and the generated CLI reference with defaults, values, conflicts, machine
  contracts, exit codes, and one example for every command.

### Distribution and contributor experience

- Add exact install, update, removal, and verification recipes for every
  package-manager and Agent Plugin client, plus practical advisory, regression,
  monorepo, fork-safe, scheduled, and promotion Action recipes.
- Record that existing static artifacts satisfy the validated editor workflows;
  no first-party extension, language server, watcher, or background detector is
  planned without a reproducible missing contract.
- Add tested changed-surface classification, contributor doctor and complete CI
  commands, stronger issue intake, real module boundaries, faster hermetic
  Action tests, lane timing, concurrency cancellation, and an aggregate hosted
  validation gate.

## 0.12.1 - 2026-08-12

This patch republishes the complete 0.12.0 payload under a recoverable GitHub
Release identity. The original `v0.12.0` tag and crates.io package remain
immutable, but its first GitHub Release record was accidentally published
without assets and GitHub permanently tombstoned the tag after that empty
record was deleted.

### Release recovery

- Preserve every 0.12.0 product, report, planning, Action, packaging, dogfood,
  and distribution contract without changing detector behavior.
- Publish the seven native archives, checksums, release manifest, CycloneDX
  and SPDX SBOMs, and Homebrew Formula together on the verified draft before
  GitHub locks `v0.12.1`.
- Keep downstream Homebrew and Scoop dispatch fail closed until the public
  release is immutable and its exact manifest and asset inventory verify.
- Document `v0.12.0` as superseded for GitHub archive and Action installation;
  crates.io consumers may retain it, while new installations should use
  0.12.1.

## 0.12.0 - 2026-08-12

This release turns the complete post-0.11.8 audit into executable product,
report, planning, Action, packaging, dogfood, and distribution contracts.

### Security and contract integrity

- Contain prompt-pack reads beneath the canonical repository, reject symlink
  and absolute-path escapes, bound streamed context reads, and keep local paths
  private unless explicitly requested.
- Make runtime report validation execute the published JSON Schema, constrain
  scalar enums, ranges, revisions, and digests identically, and exercise the
  complete malformed-value matrix in both local and packaged qualification.
- Publish build-info schema 2 with target, crate digest, compiler, and build
  source identity; pin every third-party Action; and add Cargo, Actions,
  advisory, license, and source-policy automation.

### Analysis and maintenance planning

- Represent generated provenance as source paths/globs plus generator and
  verification commands, including configuration for commentless generated
  formats.
- Separate anchor evidence from relationship confidence, keep weak edges as
  investigation context, generate focused path/module/generator verification,
  and replace score-gaming targets with reason-code and maintenance-band
  outcomes.
- Standardize policy, intervention, advisory, annotation, and SARIF scopes;
  bound relationship graphs by report profile; improve mechanical-import,
  test-mapping, fixture affinity, and concept extraction evidence.

### User, CI, and release experience

- Repair portable HTML deep links and missing-record feedback, scope scan locks
  to state roots, compress and content-address named baselines, support custom
  baseline state directories, and add consistent report-validation output.
- Lock packaged-schema tooling, preflight complete history, publish the
  release-manifest schema and exact release/attestation verification commands,
  and replace snapshot release statuses with live verification surfaces.
- Turn self-dogfood into an intentional absolute and named-baseline regression
  gate, add Action mode presets and alias removal dates, expose cargo-binstall
  metadata, and require immutable downstream Homebrew bottle releases.

## 0.11.8 - 2026-08-11

This patch makes every published count, gate, schema, and maintenance plan use
the canonical detector contract, then turns the release audit into executable
regression coverage.

### Canonical policy and machine contracts

- Make the Action consume non-failing canonical `git slop check` JSON, report
  the annotations actually emitted, and keep compact reports enforcement-safe
  through exhaustive file and folder policy indexes.
- Repair and immutably pin the Codex dependency workflow, contain upstream
  failures, validate Action inputs in `xtask`, and keep repository mutation
  credentials off acquisition and diagnostic steps.
- Finish strict report-schema definitions, format-aware external validation,
  negative runtime/schema parity fixtures, a typed baseline schema, generated
  CLI-reference drift checks, and read-only packaged-validator isolation.
- Correct unchanged comparison pagination, publish classification-aware SARIF,
  structured report violations, exact CLI pointers, private-by-default report
  descriptors, and strict cache-quarantine discovery.

### Evidence, plans, and presentation

- Calibrate structural eligibility, symbol extraction, relationship support and
  lower bounds, diffusion evidence, Rust test mapping, verification totals,
  classifications, synchronization groups, and intervention versus
  investigation rankings.
- Preserve nearby tests and focused verification commands in plans; replace
  arbitrary score reductions with threshold or reason-code outcomes; redirect
  generated, fixture, and vendored work to controllable sources; and add
  idempotent baseline ensure semantics.
- Separate prompt planning usability from execution readiness, prioritize
  complete target sources and tests, remove local paths by default, and repair
  HTML search, filters, keyboard navigation, truncation, relationship evidence,
  and cross-view file links.

### Release and maintainability

- Publish an explicit non-circular release trust graph, direct PowerShell
  verification, downstream workflow status links, correct signed-tag guidance,
  trusted-publishing requirements, and a fail-closed Homebrew bottle
  immutability contract.
- Split Action runtime and publication behavior from orchestration, add bounded
  performance gates for 1k, 30k, and 100k path scales, and codify self-dogfood
  expectations for policy parity, verification mapping, generated-document
  drift, relationship plans, and structural rankings.

## 0.11.7 - 2026-08-10

This patch makes GitHub's platform-enforced release immutability part of the
public distribution contract rather than relying on provenance checks alone.

### Release immutability

- Require every stable public release to report `immutable: true` before the
  post-publication verifier can dispatch the Scoop update.
- Require the same locked release identity for Homebrew recovery and for any
  rerun that encounters an already-published GitHub Release.
- Preserve draft-first assembly so all archives, checksums, manifests, SBOMs,
  and the Formula are complete before GitHub locks the exact tag and assets.
- Validate these invariants structurally with `cargo xtask`, document
  patch-forward recovery, and surface the GitHub release attestation in the
  publication summary.

## 0.11.6 - 2026-08-10

This patch makes the published machine contracts independently valid and
closes the post-0.11.5 audit across evidence readiness, automation, release
integrity, detector policy, planning, presentation, cache, and storage.

### Contract and automation integrity

- Separate expected non-text records from real coverage loss, share one
  fail-closed readiness evaluator, and give every tracked record a raw-byte
  SHA-256 identity.
- Align every published schema with emitted output, restore compact reports,
  emit physical-line NDJSON, and bind prompt packs to exact source bytes.
- Contain every Action consumer failure, reserve `check` for enforcement,
  expose compressed artifacts, and refresh identity-keyed token caches.
- Emit valid CycloneDX licenses, Cargo.lock hashes, standard scopes, richer
  release manifests and notes, and independently exercise packaged outputs.

### Detector and maintainer experience

- Make policy and queue behavior classification-aware, distinguish generated
  and fixture investigation from source intervention, group mechanical release
  synchronization, and expose calibrated relationship evidence.
- Make baselines comparison-ready by default, enrich inspection and formats,
  improve compare pagination/privacy, and produce report-bound idempotent plans
  with source-controllable acceptance criteria.
- Repair prompt-pack/HTML completeness and provenance, reject meaningless CLI
  filters, unify typed scope/config errors, and harden cache/report recovery and
  retention accounting.

## 0.11.5 - 2026-08-10

This patch supersedes 0.11.4 before GitHub Release publication. Version 0.11.4
remains immutable on crates.io and as a signed tag.

### Evidence completeness

- Treat binary files, Git submodules, and undecodable non-text files as
  intentionally outside structural policy evaluation instead of reporting
  them as missing evidence.
- Keep policy checks and comparisons fail-closed for actual coverage loss,
  including missing paths, configured large-file limits, degraded analysis,
  and unknown inventory states.
- Exercise both the accepted non-text boundary and rejected coverage-loss
  boundary in generated-report and comparison regression tests.

## 0.11.4 - 2026-08-10 (superseded before GitHub Release publication)

### Action reliability

- Preserve each file's analysis profile in generated action-queue entries so
  reports satisfy their own schema and the composite Action can validate a
  real repository scan.
- Validate a freshly generated non-empty report in the regression suite,
  closing the gap that allowed candidate packaging to pass while the final
  release Action smoke failed.

## 0.11.3 - 2026-08-10

This release preserves the complete 0.11.2 audit-hardening contract while
decomposing the largest maintainer and runtime hotspots identified by the
tool's own full-evidence scan. Version 0.11.2 was never published and is
superseded by this candidate.

### Maintainability

- CLI execution is divided into command-focused analysis, check, comparison,
  reporting, listing/cache, generation, and shared-support units while one
  parser, dispatcher, and typed error adapter retain the public contract.
- Report writing is divided into profile shaping, schema/migration, recursive
  validation, and atomic storage units. Analysis separates token caching,
  structural evidence, execution, and regression tests; history separates log
  parsing, Git streaming, rename lineage, metrics, execution, and fixtures.
- Action installation is divided into release API, manifest, archive/crate,
  binary-verification, and tool-cache modules behind a small orchestrator, with
  the stateful integration test divided into bounded behavioral scenarios.
- Release workflow validation is divided by publication job and receiver/CI
  concern. The public workflow is generated byte-for-byte from ten ordered,
  independently reviewed stage fragments, with repository validation rejecting
  any generated-output drift.
- Preserve the public CLI, report, Action, release-manifest, and downstream
  receiver contracts through generated documentation and regression tests.

## 0.11.2 - Unreleased (superseded by 0.11.3)

This patch closes the post-0.11.1 adversarial audit around comparison safety,
hostile repository text, evidence reliability, Action state, and distribution
contracts. It also makes release reconciliation manifest-driven so adding a
new required asset no longer requires duplicated filename lists throughout
the publication pipeline.

### Trustworthy comparison and evidence

- Compact reports now retain an exhaustive comparison index, and every
  finding, queue entry, relationship endpoint, and next command resolves to a
  retained record. Standard and full-evidence profiles now have distinct
  bounded-versus-complete evidence semantics.
- Comparison rejects incomplete or degraded evidence by default, detects
  renames through unique content identity, distinguishes source changes from
  evidence-only drift, records explicit folder semantics, and supports
  `--policy-from base|head`.
- Analysis adds a shared `--as-of` clock, CRLF normalization, structured
  skipped-file evidence, conservative large-file estimates, history
  reliability shrinkage, confidence-qualified action queues, calibrated
  relationship confidence, and raw/retained/suppressed incident counts.
- Repository estimates now include runtime overhead and expose ranges and
  confidence. Cache corruption is quarantined and analysis continues
  uncached; the packed cache also has schema checks, WAL, a busy timeout,
  bounded access updates, physical-size reporting, and compact pruning.

### Machine and human contracts

- All GitHub-command, Markdown, terminal, plan, explain, and legacy renderers
  make hostile control characters visible; Action annotations escape command
  delimiters and PR-comment updates require the expected bot author.
- New versioned schemas cover errors, estimates, cache status and pruning,
  report pruning, and compare NDJSON. Compare, list, SARIF, doctor, baseline,
  prompt-pack, and HTML contracts now expose their real completeness and
  pagination semantics.
- Plans require a measurable improvement, distinguish investigation from
  intervention, carry relationship paths and discovered verification
  commands, and use first-class Git-private baseline lifecycle commands.
- The CLI reference and man page are generated from Clap, and validation
  rejects any argument whose generated description is blank.

### Action and release hardening

- The Action uses `RUNNER_TEMP` for mutable state, exposes unambiguous health,
  absolute-policy, regression, and selected-policy counts, supports an opt-in
  token cache, verifies an explicit tool-cache identity, and validates
  baseline completeness and ancestry before enforcement.
- The schema-3 release manifest inventories native archives, Formula, and both
  SBOMs by role, media type, required status, contract version, digest, size,
  and URL. Workflows and downstream handoffs derive their expected inventory
  from those roles instead of duplicating exact filename lists.
- `cargo xtask release-prepare --check-only` now rejects every modified or
  untracked path before reporting a candidate revision, preventing a dirty
  worktree from being mistaken for the exact release source.
- CycloneDX and SPDX generation uses deterministic release identity, portable
  package references, dependency scopes, and validated dependency graphs.
  Installation documentation pins the full OpenPGP fingerprint and provides
  executable key retrieval and tag-verification instructions.
- Release documentation includes a surface matrix and idempotent Homebrew and
  Scoop reconciliation commands. The repaired 0.11.1 receiver runs completed
  successfully before this candidate was prepared.

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
