# Agent Surface

This directory carries two separate plugin contracts:

- the tracked marketplace-source contract for the installed shared
  project-management plugin
- the local marketplace that publishes the `git-slop` Codex plugin

Use these surfaces:

- `AGENTS.md`: always-on repo policy
- `.agents/plugins/marketplace-source.json`: pinned marketplace source manifest
- `.agents/plugins/marketplace.json`: local publication manifest for the `git-slop` Codex plugin
- `.codex/README.md`: Codex runtime map

`git-slop` consumes the `project-management-workflows` plugin from
`coreycoto/agent-plugins` through this pinned manifest. The consumer pins the
publisher source revision, release identity, Linux target, archive member, and
SHA-256 digest. `scripts/with-agent-plugins.sh --prepare` downloads the private
PEX SCIE into an ephemeral per-job directory, and a separate `--verify` rejects
release metadata, embedded revision, digest, or archive-safety mismatches before
execution. The read token exists only during acquisition. The canonical
commands are `marketplace`, `github project-snapshot`, and
`github execution-state`; PEX interpreter mode only preserves legacy Python
entry-point compatibility.

The verified runtime embeds the marketplace payload, so later marketplace
installation is offline and no command clones the publisher repository.
Execution-state governance commands still use the resolved project token for
their intended API calls, but that PAT is scoped only to the two direct
operation steps. It is absent from preparation, verification, and the publisher
identity and interpreter smoke checks. No Actions cache, system Python setup,
`uv`, or consumer dependency environment is part of this contract. Bootstrap
implementation, reusable behavior tests, and clean-room consumer smoke coverage
stay in `agent-plugins`, not this consumer repository.

Repo-owned validation belongs to the private standalone Rust `xtask/` workspace. Do not add
a consumer Python project, project dependency sync, or duplicated publisher
implementation here.

`git-slop` also publishes its repo-local Codex plugin from `plugins/git-slop`.
That plugin owns product-specific guidance for installing, running,
interpreting, planning from, and adopting the `git-slop` CLI. It should
reference `project-management-workflows` only when reviewed `git-slop` output is
being converted into backlog or governance work.
