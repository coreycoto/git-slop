# Agent Surface

This directory only carries the tracked marketplace-source contract for the
installed project-management plugin.

Use these surfaces:

- `AGENTS.md`: always-on repo policy
- `.agents/plugins/marketplace-source.json`: pinned marketplace source manifest
- `.codex/README.md`: Codex runtime map

`git-slop` consumes the `project-management-workflows` plugin from
`coreycoto/agent-plugins` through a pinned marketplace-source bootstrap helper
that runs `codex marketplace add <source> --ref <sha>` or writes the equivalent
home config when Codex is unavailable on PATH. That plugin is the only
supported skill surface for these maintainer workflows.
