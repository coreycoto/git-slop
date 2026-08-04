# Architecture

## Design Goals

Git Slop is a native Rust CLI built to remain deterministic, inspectable, and
local-first. The product runtime:

- inventories only files tracked by Git
- derives context, history, and structural facts locally
- keeps stable hotspot costs separate from additive overlay evidence
- writes one versioned machine contract and several derived human surfaces
- never needs a hosted API or model provider to analyze a repository

Git itself is the only required runtime dependency.

## Runtime Pipeline

```text
Git worktree
  -> tracked-file inventory
  -> context and structural tokens
  -> Git history facts
  -> stable hotspot scoring
  -> additive overlay analyzers
  -> repository-health rollups
  -> schema-4 report assembly
  -> atomic latest and timestamped bundles
  -> terminal / Markdown / JSON / YAML / SARIF / GitHub annotations
```

`find` owns the pipeline. All other analysis commands consume an existing
schema-4 report and do not rerun or rescore the detector.

## Rust Module Layout

```text
src/
  main.rs
  lib.rs
  cli.rs
  analyze.rs
  config.rs
  git.rs
  inventory.rs
  history.rs
  scoring.rs
  overlays.rs
  overlays/
    common.rs
    coordination.rs
    relationships.rs
    clusters.rs
    folders.rs
  health.rs
  health/
    model.rs
    rollup.rs
    render.rs
    tests.rs
  report.rs
  report/
    assembly.rs
    render.rs
    support.rs
    write.rs
  report_ops.rs
  report_ops/
    compare.rs
    explain/
      mod.rs
      render.rs
    github.rs
    plan/
      mod.rs
      rank.rs
    sarif.rs
  model.rs
```

### Entry Point And CLI

- `main.rs` delegates to the library CLI and returns its process exit code.
- `cli.rs` defines the `clap` command surface, validates selectors and
  thresholds, resolves report paths, and dispatches read-only artifact
  operations.
- `lib.rs` exposes the product modules and the Cargo package version.

### Detector Pipeline

- `analyze.rs` orchestrates one detector run, initializes the
  `cl100k_base` tokenizer, assembles file facts, applies scoring and overlays,
  derives health rollups, and writes the report bundle.
- `git.rs` resolves repository metadata, lists tracked files, and provides
  bounded Git queries.
- `inventory.rs` reads tracked text files, applies configured ignore globs,
  rejects binary or undecodable files, classifies paths, and counts lines.
- `history.rs` mines deterministic age, revision, churn, authorship, and
  co-change facts from local Git history.
- `scoring.rs` owns context bands, stable maintenance-pressure scoring, reason
  codes, and folder aggregation.
- `overlays.rs` orchestrates organization, verification, navigation,
  blast-radius, stewardship, and semantic-drift evidence without changing
  `slop_score`; focused analyzers live under `overlays/`.

### Reports And Read-Only Operations

- `health.rs` exposes repository-health analysis; typed rollups and Markdown,
  annotation, and JSON rendering live under `health/`.
- `report.rs` exposes report construction and persistence; focused schema
  assembly, rendering, and atomic bundle writing live under `report/`.
- `report_ops.rs` owns shared report readers and selectors; focused
  `compare`, `explain`, `plan`, `sarif`, and GitHub projections live under
  `report_ops/`.
- `config.rs` loads and normalizes config schema 2, including the one-cycle
  schema-1 compatibility path, and owns `.slop/` state paths.
- `model.rs` contains typed facts shared by pipeline stages.

## Facts And Token Systems

The pipeline keeps two token representations:

### Context Tokens

Context-token counts use the Rust `tiktoken-rs` implementation of
`cl100k_base`. They drive load pressure, file context bands, health bands, and
folder token rollups.

### Structural Tokens

Structural tokens use deterministic lexical and path normalization:

- Unicode NFKC normalization
- camel-case, separator, and path-segment splitting
- number normalization
- quoted-string normalization
- lowercase term extraction

They support duplication, cohesion, navigation, coupling, and drift evidence.
They never replace context-token counts in stable scoring.

## Stable Costs And Additive Evidence

The stable cost families are:

- load
- volatility
- coordination

They produce `slop_score`, `slop_band`, `context_band`, reason codes, and the
action queue.

Always-on overlay families are:

- organization health
- verification
- navigation
- blast radius
- stewardship
- semantic drift

Overlays explain adjacent risk and maintenance pressure. They do not inflate
`slop_score`, alter `slop_band`, or silently change `git slop check`.

Repository-health rollups are also additive. They turn the same report facts
into distribution tables, watchlists, and next-command recommendations for
humans and CI.

## Report Contract

The current machine report uses:

- `schema_version: 4`

Canonical top-level sections are:

- `summary`
- `repo`
- `config`
- `stats`
- `files`
- `folders`
- `action_queue`
- `costs`
- `overlays`
- `health`

For one compatibility cycle, reports also emit top-level
`organization_metrics`, `relationships`, and `clusters` mirrors. Consumers
should use canonical sections for new integrations.

`find` writes the same report as JSON and YAML, plus two Markdown projections:

- `summary.md` for detailed detector and overlay evidence
- `health.md` for a concise repository-health dashboard

## Config Contract

`.slop/config.yaml` uses:

- `schema_version: 2`

Current namespaces are:

- `inventory`
- `tokenization`
- `history`
- `scoring`
- `organization`
- `verification`
- `navigation`
- `blast_radius`
- `stewardship`
- `semantic_drift`
- `health`
- `check`

Legacy schema-1 configs are normalized in memory for one compatibility cycle.

## State And Caching

`git slop init` creates `.slop/latest/`, `.slop/runs/`, and `.slop/cache/`.
`find` atomically replaces the latest four-file bundle and writes a timestamped
copy under `.slop/runs/`.

`.slop/cache/` is reserved for deterministic performance optimizations. Cache
contents are generated state and must never be required for correctness.

## CLI And CI Boundaries

The CLI exposes:

- `git slop init`
- `git slop find`
- `git slop show`
- `git slop explain`
- `git slop plan`
- `git slop check`
- `git slop compare`
- `git slop sarif`
- `git slop health`
- `git slop version`

`find` is the only command that performs detector analysis. `show`, `explain`,
`plan`, `check`, `sarif`, and `health` consume one report; `compare` consumes
two. Prompt packs are explicit local outputs from `explain` and `plan`.

The composite GitHub Action installs a checksummed prebuilt binary, runs `find`
once, publishes `health.md` to the job summary, and then optionally renders
annotations, uploads an allowlisted artifact, comments on a pull request, or
applies the stable `check` gate.

## Rust Maintainer Surface

The public runtime and release artifacts are Rust. Accepted analyzer, CLI,
report, explain, plan, comparison, and SARIF behavior is covered by native Rust
tests and language-neutral historical report fixtures. The duplicated Python
engine was retired before the 0.9.0 release.

The private, non-publishable standalone Rust workspace under `xtask/` owns
repo-local Codex, plugin, workflow, repository, and release-contract
validation. The root workspace excludes it, and the public `git-slop` package
and native archives do not contain it. Its separate committed lockfile pins the
maintainer dependency graph. New product runtime behavior and repository-owned
maintainer automation must be implemented and tested in Rust.

The only retained Python execution is inside the separately published,
manifest-pinned `agent-plugins` PEX SCIE used by private maintainer workflows.
The consumer pins its release, source revision, target, archive member, and
SHA-256 digest. `scripts/with-agent-plugins.sh --prepare` acquires it into a
per-job directory under `RUNNER_TEMP`; `--verify` independently checks release
metadata, safe archive extraction, digest, target, and embedded revision before
execution. The acquisition token is unavailable to later commands, and there
is no cross-job Actions cache.

The SCIE embeds the marketplace payload and Python runtime, so `marketplace`
installation is offline after preparation and canonical `github
project-snapshot` and `github execution-state` commands need no system Python,
`uv`, or publisher Git checkout. Those GitHub commands retain the workflow's
GitHub token for their intended API calls. `PEX_INTERPRETER=1` is a wrapper
compatibility path for legacy Python entry points, not the primary workflow
surface. Pull-request jobs prepare from trusted base tooling and skip forks when
the private secret cannot be provided safely. Public Git Slop release jobs never
acquire this private runtime. No Python project or publisher implementation is
stored in this repository.

Execution-state's project credential is step-scoped. Runtime acquisition
receives only the publisher read token; verification and identity/interpreter
smoke receive no project token; direct project and execution-state commands
receive the resolved PAT only for their own step. In the privileged
dependency-remediation flow, the base checkout is validated and its Codex
config, profiles, agents, prompt, and schema are copied under `RUNNER_TEMP`
before the requested head is checked out. The later Codex action consumes only
those trusted control files, and only that mutation step receives
`github.token`.
