# Private maintainer xtask

`xtask` is repository-private Rust automation. It is a separate unpublished
Cargo workspace so neither its source nor its dependency graph is included in
the public `git-slop` crate, binary, or release archives.

Run it from the repository root through the Cargo alias:

```bash
cargo xtask validate
cargo xtask validate-codex
cargo xtask validate-workflows
cargo xtask check-issue-forms
cargo xtask check-distribution
cargo xtask release-prepare --version 0.9.2 --check-only
cargo xtask release-prepare --version 0.9.2
cargo xtask verify-crate \
  --crate-file dist/git-slop-0.9.2.crate \
  --version 0.9.2 \
  --revision <40-character-lowercase-commit> \
  --expected-sha256 <64-character-lowercase-sha256> \
  --output dist/crate-source.json
cargo xtask release-manifest \
  --dist-dir dist \
  --crate-source dist/crate-source.json \
  --tag v0.9.2
cargo xtask homebrew-formula \
  --manifest dist/release-manifest.json \
  --formula ../homebrew-tap/Formula/git-slop.rb
```

The validation commands are read-only. `release-prepare` accepts an exact
candidate `HEAD` before its future tag exists, runs local Rust quality,
packaging, and crates.io dry-run gates, and performs no publication. It never
creates or pushes a tag, publishes a crate, mutates a GitHub release, renders a
formula, or writes another repository.

After the protected workflow publishes the crate, `verify-crate` checks the
downloaded `.crate` checksum, exact package archive boundary, Cargo package
name/version, and clean Cargo VCS revision before writing the canonical
crates.io source record. `release-manifest` binds that source record to the
exact release tag and five native archives, writes only its declared manifest
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
