# Merge On Green

You are running in GitHub Actions inside the `git-slop` repository.

Use the custom agent `merge_gatekeeper` defined at
`.codex/agents/merge-gatekeeper.toml`. If that agent is unavailable, stop
immediately with an actionable error that names the missing agent file.
Use that agent for the read-only gate review first; only if it concludes the PR
is merge-safe may the parent run execute the final `gh pr merge`.
Treat `plugins/project-management-workflows/skills/merge-on-green/SKILL.md` as
the canonical workflow contract for this job.

## Read First

- `AGENTS.md`
- `.codex/README.md`
- `plugins/project-management-workflows/skills/_shared/references/github-mutation-contract.md`
- `plugins/project-management-workflows/skills/merge-on-green/SKILL.md`

## Goal

Merge an eligible PR only when every merge gate is already green and the PR
matches the trusted automation policy.

## Boundaries

- Use checked-out repo files, `gh`, GitHub tokens, and local CLI tooling only.
- Do not assume Marketplace-installed connectors are available on the runner.
- Do not use the GitHub Git Data API.
- Never edit repo files in this workflow.
- Only consider PRs authored by trusted bots or branches matching `codex/*`.
- Require the label `auto-merge`.
- If any gate fails or no eligible PR is attached to the triggering workflow
  run, return `status = "noop"` or `status = "blocked"` and do not merge.

## Workflow

1. Identify the PR associated with the triggering workflow run.
2. Verify checks are green, the branch is fresh, review state is merge-safe,
   and the required label is present.
3. Merge with standard `gh pr merge` only if every gate passes.
4. Report the exact gates that passed or blocked the merge.

Your final response must satisfy the structured output schema for this workflow.
