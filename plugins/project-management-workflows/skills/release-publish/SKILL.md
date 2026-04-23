---
name: "release-publish"
description: "Use this skill when a semver tag should be turned into a GitHub release with standard build and publication flows."
---

# Release Publish

Use this skill to prepare release notes, build release artifacts, and publish
the GitHub release through standard `git` and `gh` flows only.

## Prerequisites

- Local interactive use requires both the official GitHub Codex plugin and the bundled GitHub connector mapping.
- Run `python3 ../../scripts/preflight_github_surface.py` if you need to confirm the combined local prerequisite before continuing.

## Read First

- `../_shared/references/github-runtime-prerequisites.md`
- `../_shared/references/github-mutation-contract.md`
- `../_shared/references/workflow-tooling-surface.md`

## Workflow

1. Resolve the current repository context with `uv run agent-tools github current-repo --format json` when repository metadata is needed.
2. Build the release artifacts with `uv build` and draft or update notes under `.artifacts/releases/`.
3. Inspect existing release state with `gh release view`.
4. Publish or update the GitHub release with standard `gh release create` and `gh release upload` flows only.
