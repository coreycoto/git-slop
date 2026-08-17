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
cargo xtask release-prepare --version 0.16.1 --check-only
cargo xtask release-prepare --version 0.16.1
cargo xtask release-status --version 0.16.1 --format json
cargo xtask advisor-capacity --help
cargo xtask advisor-benchmark --help
cargo xtask advisor-benchmark-finalize --help
cargo xtask verify-crate \
  --crate-file dist/git-slop-0.16.1.crate \
  --version 0.16.1 \
  --revision <40-character-lowercase-commit> \
  --expected-sha256 <64-character-lowercase-sha256> \
  --output dist/crate-source.json
cargo xtask release-manifest \
  --dist-dir dist \
  --crate-source dist/crate-source.json \
  --tag v0.16.1
cargo xtask homebrew-formula \
  --manifest dist/release-manifest.json \
  --formula ../homebrew-tap/Formula/git-slop.rb
```

`release-publish.yml` is generated from the ordered stage fragments under
`.github/workflow-sources/release-publish/`. Edit the smallest applicable
fragment, run `cargo xtask generate-release-workflow`, and validate the exact
generated workflow before review.

The validation commands are read-only. `release-prepare` accepts an exact
candidate `HEAD` before its future tag exists, runs local Rust quality,
packaging, and crates.io dry-run gates, and performs no publication. It never
creates or pushes a tag, publishes a crate, mutates a GitHub release, renders a
formula, or writes another repository.

`ci --quiet` suppresses successful gate subprocess output while retaining a
useful failure. `ci --format json` implies quiet mode and emits one terminal
receipt with passed gates, the failed gate when applicable, elapsed time, and
the stable status.

`advisor-benchmark` owns the reproducible, privacy-safe local Safeguard matrix.
An explicit review directory must be absolute and outside this repository;
validated case artifacts written there remain private. After review,
`advisor-benchmark-finalize` binds the private ratings digest to an existing
completed result and recalculates its manual gates without rerunning inference.

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
maintainer memory.

After the protected workflow publishes the crate, `verify-crate` checks the
downloaded `.crate` checksum, exact package archive boundary, Cargo package
name/version, and clean Cargo VCS revision before writing the canonical
crates.io source record. `release-manifest` binds that source record to the
exact release tag and seven native archives, writes only its declared manifest
and checksum outputs, and includes `release-manifest.json` in `SHA256SUMS`.
`homebrew-formula` accepts only a fully valid release manifest and writes only
the declared formula path; the rendered formula builds from the immutable
`static.crates.io` URL and SHA-256 in that manifest.

The separately published `agent-plugins` maintainer runtime is an eager Linux
x86_64 PEX SCIE invoked only by trusted Codex/governance workflows through
`scripts/with-agent-plugins.sh`. The consumer manifest pins its source revision,
release coordinates, archive member, byte size, and SHA-256. Workflows acquire
it into an ephemeral `RUNNER_TEMP` root with a step-scoped
`AGENT_PLUGINS_READ_TOKEN`, verify it again without credentials, and never put
it in an Actions cache. Direct `marketplace` and `github` CLI commands are the
normal interface; the wrapper confines interpreter mode to runtime identity,
embedded-marketplace provenance verification, and the legacy compatibility
entry point. The runtime is not part of this xtask or the public `git-slop`
product runtime.

`validate-codex` and `validate-workflows` fail closed on malformed runtime pins,
legacy source or dependency acquisition, implicit downloads, misplaced
acquisition credentials, persistent cache use, indirect CLI shims, unsafe
pull-request checkout ordering, or coupling the private runtime to public
release publication.
