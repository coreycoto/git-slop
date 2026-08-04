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
`coreycoto/agent-plugins` through this pinned manifest. The publisher-owned
`agent_plugins.marketplace.bootstrap` module installs into isolated Codex homes
through `scripts/with-agent-plugins.sh`. That wrapper resolves the exact URL and
40-character source revision from the manifest with an isolated
`uv run --no-project`; bootstrap implementation, reusable behavior tests, and
clean-room consumer smoke coverage stay in `agent-plugins`, not this consumer
repository.

Repo-owned validation belongs to the private standalone Rust `xtask/` workspace. Do not add
a consumer Python project, project dependency sync, or duplicated publisher
implementation here.

`git-slop` also publishes its repo-local Codex plugin from `plugins/git-slop`.
That plugin owns product-specific guidance for installing, running,
interpreting, planning from, and adopting the `git-slop` CLI. It should
reference `project-management-workflows` only when reviewed `git-slop` output is
being converted into backlog or governance work.
