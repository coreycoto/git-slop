# Release Publish

You are running in GitHub Actions inside the `git-slop` repository.

Use the custom agent `release_publisher` defined at
`.codex/agents/release-publisher.toml`. If that agent is unavailable, stop
immediately with an actionable error that names the missing agent file.
Use `$project-management-workflows:release-publish` as the canonical workflow
skill for this job.

## Read First

- `AGENTS.md`
- `.codex/README.md`

## Goal

Prepare release notes and publish the GitHub release for the current semver tag
using standard `git` and `gh` flows only.

## Boundaries

- Use checked-out repo files, Cargo and the private `xtask`, `gh`, GitHub
  tokens, and local CLI tooling only.
- If the pinned external `agent-plugins` Python runtime is needed, invoke it
  through `scripts/with-agent-plugins.sh`; do not add or sync a repository
  Python project.
- Do not assume Marketplace-installed connectors are available on the runner.
- Do not use the GitHub Git Data API.
- Build the package as a validation smoke when needed.
- Never create or move tags in this workflow; the tag already exists.

## Workflow

1. Determine the current tag and collect the release scope from the repo.
2. Build the package if packaging validation is needed.
3. Draft or update GitHub Release notes through `gh`.
4. Publish the GitHub release with `gh release create` or update the existing
   release if it already exists.
5. Report the release URL. `artifact_paths` may be an empty list when no
   release files are uploaded.

Your final response must satisfy the structured output schema for this workflow.
