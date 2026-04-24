# Agent Surface

This directory only carries the tracked marketplace-source contract for the
installed project-management plugin.

Use these surfaces:

- `AGENTS.md`: always-on repo policy
- `.agents/plugins/marketplace-source.json`: pinned marketplace source manifest
- `.codex/README.md`: Codex runtime map

`git-slop` consumes the `project-management-workflows` plugin from
`coreycoto/agent-plugins` through a pinned marketplace-source bootstrap helper
that registers the marketplace source, enables
`project-management-workflows@agent-plugins-marketplace`, and materializes the
pinned plugin into the local Codex plugin cache. That plugin is the only
supported skill surface for these maintainer workflows.
