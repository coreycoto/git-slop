# Private maintainer xtask

`xtask` is repository-private Rust automation. It is a separate unpublished
Cargo workspace so neither its source nor its dependency graph is included in
the public `git-slop` crate, binary, or release archives.

Run it from the repository root through the Cargo alias:

```bash
cargo xtask validate
cargo xtask ci --quiet
cargo xtask ci --format json
cargo xtask validate-codex
cargo xtask validate-workflows
cargo xtask generate-release-workflow --check
cargo xtask check-issue-forms
cargo xtask check-distribution
cargo xtask release-prepare --version 0.16.5 --check-only
cargo xtask release-prepare --version 0.16.5 --check-only --require-release-date
cargo xtask release-prepare --version 0.16.5
cargo xtask release-status --version 0.16.5 --format json
cargo xtask advisor-capacity --help
cargo xtask advisor-benchmark --help
cargo xtask advisor-benchmark-finalize --help
cargo xtask verify-crate \
  --crate-file dist/git-slop-0.16.5.crate \
  --version 0.16.5 \
  --revision <40-character-lowercase-commit> \
  --expected-sha256 <64-character-lowercase-sha256> \
  --output dist/crate-source.json
cargo xtask release-manifest \
  --dist-dir dist \
  --crate-source dist/crate-source.json \
  --tag v0.16.5
cargo xtask homebrew-formula \
  --manifest dist/release-manifest.json \
  --formula ../homebrew-tap/Formula/git-slop.rb
```

`release-publish.yml` is generated from the ordered stage fragments under
`.github/workflow-sources/release-publish/`. Edit the smallest applicable
fragment, run `cargo xtask generate-release-workflow`, and validate the exact
generated workflow before review.

Dependabot groups routine Cargo minor and patch updates, but keeps
`tiktoken-rs` independent because its pre-1.0 releases can change tokenizer
behavior or the supported Rust floor. It ignores
`Homebrew/actions/setup-homebrew` because that pin lives in a release-workflow
source fragment; update the fragment and regenerate the workflow as one change.

The validation commands are read-only. `release-prepare` accepts an exact
candidate `HEAD` before its future tag exists, runs local Rust quality,
packaging, and crates.io dry-run gates, and performs no publication. It never
creates or pushes a tag, publishes a crate, mutates a GitHub release, renders a
formula, or writes another repository.

Local preparation accepts an `Unreleased` changelog heading. The protected
publication workflow adds `--require-release-date` and fails before crate
publication unless the exact candidate commit has a `YYYY-MM-DD` heading. The
same validation checks the release note's declared improvement total,
contiguous numbering, version heading, and changelog link.

`ci --quiet` suppresses successful gate subprocess output while retaining a
useful failure. `ci --format json` implies quiet mode and emits one terminal
receipt with passed gates, the failed gate when applicable, elapsed time, and
the stable status.

`advisor-benchmark` owns the reproducible, privacy-safe local Safeguard matrix.
An explicit review directory must be absolute and outside this repository;
validated case artifacts written there remain private. After review,
`advisor-benchmark-finalize` binds the private ratings digest to an existing
completed result without rerunning inference. It is the only ratings entry
point. The finalizer schema-validates the result, verifies its source digests
and thresholds, binds the exact matrix and repository evidence to the pinned
corpus, recomputes its recommendation and automatic gates from samples, and
refuses already finalized evidence before adding manual gates. It also requires
the current decision report to match its JSON result and regenerates the report
from the finalized result.

Its implementation is split into real Rust modules under
`src/advisor_benchmark/`: `run` owns orchestration, `system` owns bounded child
execution and resource guards, `review` owns blinded evidence and ratings,
`derivation` is the single decision engine, and `finalization` verifies and
persists immutable reviewed results. Keep cross-module contracts typed instead
of recreating string-state branches in the CLI.

`advisor-capacity` is the provider-free first gate for proposed benchmark
hardware. It reads only physical memory, available memory, and swap, never
reads a report or contacts a provider, and emits a receipt that states both
facts. An ineligible host exits nonzero after printing every blocker rather
than only the first one. The JSON receipt follows
`git slop schema advisor-capacity`; human output shows the same complete limit
contract. Run this before building the inference feature or provisioning a
runtime; never replace it with the full benchmark on a low-memory development
machine.

The benchmark retains at most 8 MiB from each child stdout/stderr stream while
continuing to drain both. Crossing either boundary terminates the matrix with a
privacy-safe incomplete result instead of deadlocking or consuming unbounded
maintainer memory. Its new temporary workspace is mode 0700 on Unix and uses a
distinct artifact path for every sample. Prepare-only, aggregate, interrupted,
and finalized results must pass the strict published `advisor-benchmark-1`
schema before `results.json` and `decision.md` are replaced as a rollback-safe
pair.

Focused no-provider tests run the child supervisor through success, nonzero
exit, deadline, forced-kill, stdout-flood, and stderr-flood cases. They are the
required regression matrix for lifecycle, byte-bound, and reap behavior; they
must never start Ollama or another model runtime.

After the protected workflow publishes the crate, `verify-crate` checks the
downloaded `.crate` checksum, exact package archive boundary, Cargo package
name/version, and clean Cargo VCS revision before writing the canonical
crates.io source record. `release-manifest` binds that source record to the
exact release tag and seven native archives, writes only its declared manifest
and checksum outputs, and includes `release-manifest.json` in `SHA256SUMS`.
`homebrew-formula` accepts only a fully valid release manifest and writes only
the declared formula path; the rendered formula builds from the immutable
`static.crates.io` URL and SHA-256 in that manifest.

Public Agent Plugins supply the selected project- and product-development
guidance. The `coreycoto/gh-steward` Go extension supplies native GitHub
snapshots, reviewed plans, exact-hash applies and durable operation journals.
Consumer locks bind both sources. `scripts/prepare-codex-plugins.sh` keeps the
Codex CLI and plugins under an isolated temporary `CODEX_HOME`;
`scripts/with-gh-steward.sh` stages a source-pinned, checksum- and
attestation-verified native tool under `RUNNER_TEMP`. Neither installs
globally or loads the private Agent Development publisher. The four platform
asset hashes and exact tool commit must be qualified before acquisition can
succeed; unqualified lock values fail closed.

Execution-state recovery persists the exact repository, target, run identity,
reviewed plan, apply receipt and native journal as a target-specific workflow
artifact. The workflow searches complete run and artifact inventories before
preparing any new plan. It resumes only from matching evidence and fails with
`recovery_needed` when evidence is missing, expired, incomplete or mismatched.
`cancel-in-progress` remains disabled to preserve dispatch state.

`validate-codex` and `validate-workflows` check immutable plugin and tool pins,
acquisition scripts, isolated setup, workflow credential scope, trusted
pull-request ordering, cross-run recovery, and public release independence
from consumer tooling.
