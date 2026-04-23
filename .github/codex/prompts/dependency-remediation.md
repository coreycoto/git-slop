# Dependency Remediation

You are running in GitHub Actions inside the `git-slop` repository.

Use the custom agent `dependency_patcher` defined at
`.codex/agents/dependency-patcher.toml`. If that agent is unavailable, stop
immediately with an actionable error that names the missing agent file.
Treat `plugins/project-management-workflows/skills/dependency-remediation/SKILL.md`
as the canonical workflow contract for this job.

## Read First

- `AGENTS.md`
- `.codex/README.md`
- `plugins/project-management-workflows/README.md`
- `plugins/project-management-workflows/skills/_shared/references/github-mutation-contract.md`
- `plugins/project-management-workflows/skills/dependency-remediation/SKILL.md`

## Goal

Own the smallest safe remediation for a trusted dependency-bot update or
security-triggered dependency issue. Prefer lockfile-only or manifest-only
changes. Expand into code edits only when the dependency update requires a
minimal compatibility fix.

## Boundaries

- Use checked-out repo files, `gh`, GitHub tokens, `agent-tools`, and local
  CLI tooling only.
- Do not assume Marketplace-installed connectors are available on the runner.
- Do not use the GitHub Git Data API.
- Keep the change narrow: dependency manifests, lockfiles, and the minimum
  directly affected source or test files.
- Run bounded verification before opening or updating a PR.
- If there is no actionable remediation, return `status = "noop"`.

## Workflow

1. Inspect the triggering event and identify the dependency/CVE scope.
2. Apply the minimum viable remediation in the checked-out workspace.
3. Run bounded verification appropriate to the changed surface.
4. If a trusted-bot PR branch is writable, reuse it; otherwise create or update
   a narrow `codex/dependency-remediation-*` branch and PR with `gh`.
5. Summarize exactly what changed and how it was verified.

Your final response must satisfy the structured output schema for this workflow.
