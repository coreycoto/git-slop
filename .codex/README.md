# Codex Runtime

This directory defines the repo-local Codex runtime surface for `git-slop`.

Use the runtime layers like this:

- `AGENTS.md`: always-on repo-global execution rules
- `.codex/config.toml`: project-scoped defaults, app permissions, and CI profiles
- `.codex/rules/*.rules`: interactive approval prompts for sensitive shell commands
- `.codex/agents/*.toml`: custom execution roles
- `.agents/plugins/marketplace-source.json`: pinned marketplace source manifest
- installed `project-management-workflows` plugin from `coreycoto/agent-plugins`: canonical reusable workflow contract
- `.github/codex/prompts/*`: workflow prompts that explicitly name the custom agents to use
- `.github/codex/schemas/*`: structured-output schemas for Codex-driven workflows

## Approval And Publication

Interactive local sessions should default to `approval_policy = "on-request"`.
Non-interactive CI profiles should use `approval_policy = "never"` and rely on
explicit workflow permissions instead of fresh approvals.

Publication rules:

- prefer `git push`, `gh release`, and `gh pr merge`
- prompt before those commands in interactive sessions
- do not fall back to GitHub Git Data API publication unless the user explicitly requests it

## Custom Agent Boundary

Custom agents are specialized workers, not always-on policy.

They should:

- stay narrow and opinionated
- match the workflow prompts that explicitly ask Codex to use them
- reference plugin-owned skills for workflow contract
- keep only role, sandbox, model, and delegation guidance
- avoid becoming the primary store for reusable workflow or repo policy

Current project-scoped agents:

- `dependency_patcher`
- `merge_gatekeeper`
- `governance_auditor`
- `docs_taxonomist`
- `release_publisher`

## Workflow Assets

Codex-driven GitHub Actions should keep their task contract in checked-in
prompt and schema files. The schema files are workflow-owned assets. They are
not auto-discovered by Codex or by custom agents; workflows must pass them
explicitly via `--output-schema`.
