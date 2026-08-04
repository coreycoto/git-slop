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

The separately pinned `agent_plugins` dependency remains Python and is invoked
only by trusted Codex/governance workflows through
`scripts/with-agent-plugins.sh`. It is not part of this xtask or the product
runtime.
