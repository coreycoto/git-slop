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
cargo xtask release-prepare --version 0.9.0 --check-only
cargo xtask release-manifest --tag v0.9.0
cargo xtask homebrew-formula --manifest dist/release-manifest.json
```

The validation commands are read-only. `release-manifest` writes only its
declared manifest and checksum outputs. `homebrew-formula` writes only the
declared formula path. `release-prepare` runs local Rust quality/package dry
runs and renders a formula; it never creates or pushes a tag, publishes a
crate, mutates a GitHub release, or runs Homebrew publication commands.

The separately published `agent-plugins` maintainer runtime is an eager Linux
x86_64 PEX SCIE invoked only by trusted Codex/governance workflows through
`scripts/with-agent-plugins.sh`. The consumer manifest pins its source revision,
release coordinates, archive member, byte size, and SHA-256. Workflows acquire
it into an ephemeral `RUNNER_TEMP` root with a step-scoped
`AGENT_PLUGINS_READ_TOKEN`, verify it again without credentials, and never put
it in an Actions cache. Direct `marketplace` and `github` CLI commands are the
normal interface; the wrapper maps compatibility `python -c` imports to the
verified runtime with `PEX_INTERPRETER=1`. The runtime is not part of this xtask
or the public `git-slop` product runtime.

`validate-codex` and `validate-workflows` fail closed on malformed runtime pins,
legacy Git/uv/Python acquisition, implicit downloads, misplaced acquisition
credentials, persistent cache use, indirect CLI shims, unsafe pull-request
checkout ordering, or coupling the private runtime to public release
publication.
