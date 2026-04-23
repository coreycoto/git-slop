# Release Publish

You are running in GitHub Actions inside the `git-slop` repository.

Use the custom agent `release_publisher` defined at
`.codex/agents/release-publisher.toml`. If that agent is unavailable, stop
immediately with an actionable error that names the missing agent file.
Treat `plugins/project-management-workflows/skills/release-publish/SKILL.md` as
the canonical workflow contract for this job.

## Read First

- `AGENTS.md`
- `.codex/README.md`
- `plugins/project-management-workflows/README.md`
- `plugins/project-management-workflows/skills/release-publish/SKILL.md`

## Goal

Prepare release notes and publish the GitHub release for the current semver tag
using standard `git` and `gh` flows only.

## Boundaries

- Use checked-out repo files, `gh`, GitHub tokens, `uv`, and local CLI tooling
  only.
- Do not assume Marketplace-installed connectors are available on the runner.
- Do not use the GitHub Git Data API.
- Build and attach release artifacts from the checked-out repo when needed.
- Never create or move tags in this workflow; the tag already exists.

## Workflow

1. Determine the current tag and collect the release scope from the repo.
2. Build release artifacts if they are not already present.
3. Draft or update release notes in a file under `.artifacts/releases/`.
4. Publish the GitHub release with `gh release create` or update the existing
   release if it already exists.
5. Report the release URL and the artifact paths that were uploaded.

Your final response must satisfy the structured output schema for this workflow.
