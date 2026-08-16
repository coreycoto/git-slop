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
  -> normalized schema-5 report assembly
  -> atomic latest and timestamped bundles
  -> terminal / Markdown / JSON / YAML / SARIF / GitHub annotations
```

`find` owns the pipeline. All other analysis commands consume an existing
schema-5 report and do not rerun or rescore the detector.

## Rust Module Layout

```text
src/
  main.rs
  lib.rs
  build_info.rs
  cli.rs
  analyze.rs
  config.rs
  git.rs
  inventory.rs
  history.rs
  scoring.rs
  overlays/
    mod.rs
    common/
      mod.rs
    coordination.rs
    relationships.rs
    clusters.rs
    folders.rs
  health.rs
  health/
    model.rs
    rollup.rs
    render/
      mod.rs
      support.rs
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
- `build_info.rs` exposes schema-2 package, target, crate, and source-build
  provenance embedded by Cargo packaging for release verification.
- `cli.rs` defines the top-level `clap` command surface and dispatches focused
  operations under `cli/`; list and cache argument contracts live in
  `cli/list_args.rs` and `cli/cache_args.rs` so command growth does not return
  the entry point to a monolith.
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
- `overlays/mod.rs` orchestrates organization, verification, navigation,
  blast-radius, stewardship, and semantic-drift evidence without changing
  `slop_score`; focused analyzers and shared test/verification/term helpers live
  under `overlays/`.

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
- concept dispersion

Overlays explain adjacent risk and maintenance pressure. They do not inflate
`slop_score`, alter `slop_band`, or silently change `git slop check`.

Repository-health rollups are also additive. They turn the same report facts
into distribution tables, watchlists, and next-command recommendations for
humans and CI.

## Report Contract

The current machine report uses:

- `schema_version: 5`

Canonical top-level sections are:

- `summary`
- `repo`
- `config`
- `stats`
- `files`
- `folders`
- `ranked_files`
- `action_queue`
- `costs`
- `overlays`
- `health`

Relationship and cluster records are stored once in the canonical organization
overlay and referenced by stable IDs. `ranked_files` is exhaustive;
`action_queue` contains only records that require attention.

`find` always writes JSON and writes YAML only when `output.yaml: true`, plus
two Markdown projections:

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
`find` atomically replaces the latest compact bundle and writes a timestamped
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
- `git slop policy`
- `git slop advise`
- `git slop check`
- `git slop compare`
- `git slop baseline`
- `git slop sarif`
- `git slop health`
- `git slop report`
- `git slop config`
- `git slop doctor`
- `git slop list`
- `git slop prune`
- `git slop cache`
- `git slop completions`
- `git slop man`
- `git slop reference`
- `git slop schema`
- `git slop html`
- `git slop version`
- `git slop build-info`

`find` is the only command that performs detector analysis. `show`, `explain`,
`plan`, `check`, `sarif`, and `health` consume one report; `compare` consumes
two. Prompt packs are explicit local outputs from `explain` and `plan`.

`policy` manages bounded data-only policy sources and an offline user cache.
`advise` reuses deterministic `explain` and `plan` evidence and adds bounded
tracked excerpts. Its public default emits provider-free JSON context; the
checked-in `defer` gate disables provider invocation outside the maintainer
benchmark. Any future inference requires explicit provider identity, a
loopback endpoint, capacity evidence, and a separately provisioned host. Its
context cache and validated artifacts live under the active state root's
`advice/` namespace, never `.slop/latest/`. The provider module is not linked
to any model runtime and neither it nor the benchmark manages Ollama.

The advice boundary has five ordered trust zones: immutable system/output
instructions, non-disableable core policy, selected third-party policy,
deterministic candidates, and untrusted repository excerpts. Published schemas
cover pack sources, locks, provider-independent input, provider responses, and
validated artifacts. Git Slop validates the model response, checks every cited
identifier against the input index, and recomputes aggregate verdicts before
writing an artifact. No advice result is read by another command implicitly.

The composite GitHub Action installs a checksummed prebuilt binary, runs `find`
once, publishes `health.md` to the job summary, and then optionally renders
annotations, uploads an allowlisted artifact, comments on a pull request, or
applies the stable `check` gate.

## Distribution And Release Identity

The stable distribution has one canonical identity: a strict version, a
full source revision, and the SHA-256 of the crates.io `.crate`. It does not
treat Homebrew, GitHub Release, or Marketplace as independent builds.

```text
exact current main
  -> explicit Release Publish dispatch authorizes crates.io and tag mutation
  -> local candidate .crate
  -> seven-target preflight
  -> branch-restricted release environment with no reviewer gate
  -> crates.io publication and local/index/static SHA equality
  -> exact v<version> tag
  -> immutable Homebrew receiver dispatch
  -> seven archives built from downloaded registry bytes
  -> schema-3 manifest + SHA256SUMS + crates-backed Formula
  -> verified draft GitHub Release
  -> manual Marketplace publication with 2FA
  -> read-only public-release identity verification
  -> immutable Scoop receiver dispatch
  -> receiver verifies public assets and opens an exact two-file tap PR
  -> exact-head two-platform bottle tests
  -> trusted-main publisher reverifies and updates the Homebrew tap
  -> trusted bucket main reverifies and opens a manifest-only Scoop PR
  -> exact-head Windows x64/ARM64 qualification
  -> governed Scoop merge and explicit exact-main qualification
```

If crates.io has already accepted those immutable bytes and `main` advances
before the tag or draft is completed, an explicit `recover` dispatch rejoins
the chain at the registry package. It is keyed by the original full revision
and crate SHA-256. The workflow separately pins its control revision to the
exact dispatch-time `main` and rechecks that it is still live `main` after the
branch-restricted job starts and at tag mutation; only the immutable release
revision may be an older ancestor. Recovery re-verifies the API/index checksum,
static package, and embedded VCS revision and passes through the same
branch-restricted environment without a reviewer gate. It cannot publish a
crate, move a tag, or derive artifacts from advanced `main`. Recovery may
execute the current trusted Action installer to inspect a numeric draft-release
ID, while the artifacts and their provenance remain bound to the historical
release revision. Marketplace readiness still
executes the full composite Action from that exact historical tag across all
seven targets; current control tooling cannot substitute for public tagged code.

The seven targets are Linux GNU x86-64 and ARM64, static Linux musl x86-64,
macOS Apple Silicon and Intel, and Windows x86-64 and ARM64. `git-slop build-info --format json` binds each packaged
binary to the full revision with `source_dirty: false`. The composite Action
downloads one of those prebuilt archives and verifies the tag, GitHub asset
digests, checksum inventory, manifest, canonical crate digest, safe archive
shape, and embedded build identity before running. It never invokes Homebrew or
compiles Rust in a consumer repository.

The Homebrew artifact is a Formula, not a cask or an alternate binary source.
It downloads the static crates.io package at the manifest's exact digest,
builds it with Rust, and checks the same embedded revision. The
dispatch-authorized, branch-restricted publication job exposes the existing
Homebrew token only while dispatching the immutable version, revision, crate
URL, and crate digest. The tap receiver waits for the exact stable public
release, derives and
reverifies its Formula and manifest digests, and cannot create a tap PR from a
draft. The PR must have the current tap `main` as its sole parent and exactly
the Formula and release metadata files. Its canonical exact-head bottle test
run triggers trusted `main` workflow code, which independently rechecks the
event, artifacts, bot PR, parent, head, and two-file boundary immediately before
publishing. The `release.published` verifier itself remains read-only; only its
dependency-ordered Scoop dispatch step receives a separate fine-grained token,
scoped to Actions dispatch in `coreycoto/scoop-bucket`. It sends no URL or
Windows hash and introduces no Actions environment approval. The trusted-main
tap publisher adds no label or environment approval. Manual
`homebrew-handoff.yml` dispatch remains an explicit branch-restricted recovery
path with no reviewer gate. The draft must not be published until the terminal
`marketplace-ready` job confirms all seven Action smoke lanes.

Scoop is an external Windows package-manager consumer, not another release
artifact or source-build job. After the stable release is public, the source
workflow hands only its verified version, numeric release ID, full revision,
and manifest digest to `coreycoto/scoop-bucket`. That repository's trusted-main
receiver independently requires the exact twelve-asset/eleven-checksum release,
rederives both Windows hashes, and tests the installed binary's full source
revision. The trusted-main receiver creates a manifest-only bucket pull request,
explicitly dispatches required native qualification for its exact head,
rechecks the single-file bot PR and successful run immediately before a
ruleset-governed merge, then explicitly qualifies the resulting exact main.
Only after that terminal exact-main proof does it compare the published
manifest, delete the consumed `automation/git-slop-v<version>` branch, and
verify the exact remote ref is absent; idempotent reruns perform the same
bounded cleanup when necessary.
The source repository never receives bucket write permission, and the bucket
never receives a release-environment or cross-repository secret.

## Rust Maintainer Surface

The public runtime, release artifacts, and repository-owned maintainer contract
validation are Rust. Native tests and language-neutral historical report
fixtures cover accepted analyzer, CLI, report, explain, plan, comparison, and
SARIF behavior.

The private, non-publishable standalone Rust workspace under `xtask/` owns
repo-local Codex, plugin, workflow, repository, and release-contract
validation. The root workspace excludes it, and the public `git-slop` package
and native archives do not contain it. Its separate committed lockfile pins the
maintainer dependency graph. Product behavior is implemented and tested in the
root Rust crate; repository-owned contract validation belongs in `xtask/`.

Private maintainer workflows use a separately published, manifest-pinned
`agent-plugins` SCIE. The consumer pins its release, source revision, target,
archive member, and SHA-256 digest. `scripts/with-agent-plugins.sh --prepare`
acquires it into a per-job directory under `RUNNER_TEMP`; `--verify`
independently checks release metadata, safe archive extraction, digest, target,
and embedded revision before execution. The acquisition token is unavailable
to later commands, and there is no cross-job Actions cache.

The SCIE embeds its marketplace payload, so installation is offline after
preparation. Canonical `marketplace`, `github project-snapshot`, and `github
execution-state` commands invoke its direct CLI. The wrapper's isolated
interpreter is confined to runtime identity, embedded-marketplace provenance
verification, and the legacy compatibility entry point. Pull-request jobs
prepare from trusted base tooling and skip forks when the private secret cannot
be provided safely. Public Git Slop release jobs never acquire this private
runtime, and no publisher implementation is stored in this repository.

Execution-state's project credential is step-scoped. Runtime acquisition
receives only the publisher read token; verification and identity/interpreter
smoke receive no project token; direct project and execution-state commands
receive the resolved PAT only for their own step. In the privileged
dependency-remediation flow, the base checkout is validated and its Codex
config, profiles, agents, prompt, and schema are copied under `RUNNER_TEMP`
before the requested head is checked out. The later Codex action consumes only
those trusted control files, and only that mutation step receives
`github.token`.
