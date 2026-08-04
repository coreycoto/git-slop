# Governance Reconcile

You are running in GitHub Actions inside the `git-slop` repository.

Use the custom agent `governance_auditor` defined at
`.codex/agents/governance-auditor.toml`. If that agent is unavailable, stop
immediately with an actionable error that names the missing agent file.
Use that agent for the read-only audit and preview phase first; if the preview
proves that an allowed deterministic auto-fix is needed, the parent run may
apply that narrow mutation surface afterward.
Use `$project-management-workflows:github-backlog-mutate`,
`$project-management-workflows:ensure-quarter-milestones`, and
`$project-management-workflows:label-palette-design` as the canonical workflow
skills for this job.

## Read First

- `AGENTS.md`
- `.codex/README.md`
- `config/github/README.md`
- `config/labels/README.md`

## Goal

Reconcile the repo-managed governance surface and emit deterministic preview
artifacts before any allowed mutation.

## Boundaries

- Use checked-out repo files, `gh`, the workflow GitHub token, the already
  prepared and verified `agent_plugins` CLI, and local CLI tooling only. The
  private acquisition token is not available to this task.
- Do not assume Marketplace-installed connectors are available on the runner.
- Do not use the GitHub Git Data API.
- Always generate preview artifacts under `.artifacts/github-governance/`
  before apply.
- Allowed auto-fixes are limited to:
  - current/next quarter milestone create or refresh
  - repo-managed label palette sync
- Never auto-mutate:
  - issue titles
  - issue bodies
  - issue milestone assignments
  - project `Priority` or `Queue Order`
  - parent/sub-issue links

## Workflow

1. Build the current governance snapshot, issue graph, milestone check, label
   preview, and summary artifacts.
2. Decide whether an allowed deterministic auto-fix is needed.
3. Apply only the allowed mutation surface, if required.
4. Verify the live GitHub state immediately after any mutation.
5. Report preview artifacts, applied mutations, and remaining manual work.

Your final response must satisfy the structured output schema for this workflow.
