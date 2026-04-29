# Repository Agent Policy

`git-slop` uses these guidance layers:

- `AGENTS.md`: always-on repo-wide policy and execution constraints
- `.codex/README.md`: Codex runtime map for config, rules, agents, prompts, and schemas
- `.agents/plugins/marketplace-source.json`: pinned marketplace source manifest
- `.agents/plugins/marketplace.json`: local `git-slop-marketplace` publication manifest
- installed `project-management-workflows` plugin from `coreycoto/agent-plugins`: reusable workflow contract and metadata
- local `git-slop` plugin under `plugins/git-slop`: product-specific usage, install, report, interpretation, planning, and adoption guidance
- `config/github/README.md`: repo-owned backlog/project overlay and seed data
- `config/labels/README.md`: repo-owned label palette overlay

## Publication Rules

- Prefer standard `git push`, `gh release`, and `gh pr merge`.
- If direct publication is blocked by runtime policy, stop and report it.
- Do not publish commits, branches, tags, or releases through the GitHub Git Data API unless the user explicitly asks for that fallback.

## Workflow Boundaries

- Keep the public `git slop` CLI focused on detector, report, explain, and plan behavior.
- Keep reusable maintainer workflow instructions in the installed project-management plugin from `coreycoto/agent-plugins`.
- Keep the local `git-slop@git-slop-marketplace` plugin focused on product-specific CLI usage and consumer adoption guidance.
- Bootstrap that plugin through the tracked marketplace source manifest instead of a checked-in repo-local marketplace file.
- Keep repo-specific overlays next to the repo-owned data they describe under `config/*/README.md`.
- Keep custom agents thin: they should reference plugin skills and only add role, sandbox, model, and delegation guidance.

## Automation Rules

- Use prompt files under `.github/codex/prompts/` for every Codex-powered workflow job.
- Use schema files under `.github/codex/schemas/` when a workflow expects structured output before applying a mutation.
- Treat the official GitHub Codex plugin as a local interactive prerequisite, not as a CI dependency.
- In CI, rely on checked-out repo files, prompt files, custom agents, `gh`, GitHub tokens, and importable `agent_plugins` runtime APIs.
