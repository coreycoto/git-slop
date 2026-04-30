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
`coreycoto/agent-plugins` through a pinned marketplace-source bootstrap helper
that registers the marketplace source, enables
`project-management-workflows@agent-plugins-marketplace`, and materializes the
pinned plugin into the local Codex plugin cache.

`git-slop` also publishes its repo-local Codex plugin from `plugins/git-slop`.
That plugin owns product-specific guidance for installing, running,
interpreting, planning from, and adopting the `git-slop` CLI. It should
reference `project-management-workflows` only when reviewed `git-slop` output is
being converted into backlog or governance work.
