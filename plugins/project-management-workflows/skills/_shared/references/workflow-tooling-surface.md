# Workflow Tooling Surface

Use the checked-in repo, deterministic `agent-tools` helpers, and standard `gh`
flows as the canonical execution surface for these plugin workflows.

## Repo Context And Snapshots

Prefer these helpers before ad hoc GitHub reads:

- `uv run agent-tools github current-repo --format json`
- `uv run agent-tools github project-snapshot --format json`
- `uv run agent-tools github issue-graph --format json`
- `uv run agent-tools github queue-snapshot --format json`

## Governance And Planning Helpers

Use the purpose-built `agent-tools` helpers when a workflow needs them:

- `uv run agent-tools github sync-label-palette --check --format json`
- `uv run agent-tools github milestone-check --format json`
- `uv run agent-tools github validate-backlog-mutations --format json`
- `uv run agent-tools github apply-backlog-mutations --format json`
- `uv run agent-tools github review-to-backlog --format json`
- `uv run agent-tools github apply-review-backlog-delta --format json`
- `uv run agent-tools github validate-quarter-plan --format json`
- `uv run agent-tools github build-quarter-plan-delta --format json`
- `uv run agent-tools github plan-to-backlog --format json`
- `uv run agent-tools github apply-quarter-plan-delta --format json`
- `uv run agent-tools research digest --format json`

## Standard GitHub And Release Flows

Use standard Git and GitHub commands for lifecycle operations that are not
covered by deterministic `agent-tools` helpers:

- `gh pr view`, `gh pr create`, `gh pr edit`, `gh pr merge`
- `gh release view`, `gh release create`, `gh release upload`
- `uv build`

Never fall back to the GitHub Git Data API unless the user explicitly requests
that non-standard path.

## Artifact Roots

Keep bulky evidence and machine-readable results under workflow-specific roots:

- `.artifacts/intake/`
- `.artifacts/review-to-backlog/`
- `.artifacts/plan-to-backlog/`
- `.artifacts/quarter-plan/`
- `.artifacts/github-governance/`
- `.artifacts/dependency-remediation/`
- `.artifacts/docs-taxonomy/`
- `.artifacts/releases/`
