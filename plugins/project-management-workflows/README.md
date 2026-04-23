# Project Management Workflows Plugin

This repo-local plugin is the `git-slop` proving ground for reusable
project-management workflows.

It packages:

- reusable maintainer workflow skills
- shared references and decision aids
- lightweight preflight guidance for external prerequisites
- the GitHub connector mapping for GitHub-touching local interactive workflows
- canonical workflow guidance for deterministic `agent-tools`, `gh`, and artifact usage

It does **not** bundle:

- the official GitHub Codex plugin
- a custom MCP server

## Prerequisites

For local interactive use, GitHub-touching skills require both:

- the official GitHub Codex plugin
- the GitHub connector mapping bundled by this plugin via `.app.json`

This plugin packages the connector mapping, but it still does not bundle the
official GitHub plugin itself. Use
`skills/_shared/references/github-runtime-prerequisites.md` and
`scripts/preflight_github_surface.py` to confirm the combined prerequisite
before a local interactive workflow attempts live GitHub reads or writes.

For CI and GitHub Actions, do not assume marketplace-installed connectors are
available. Use checked-out repo files, prompt files, custom agents, `gh`,
GitHub tokens, `agent-tools`, and repo scripts instead.

## Structure

- `.codex-plugin/plugin.json`: plugin manifest
- `.app.json`: bundled GitHub connector mapping for local interactive use
- `skills/`: reusable workflow skills
- `skills/_shared/references/`: reusable decision aids and policy references
- `scripts/preflight_github_surface.py`: local preflight for the combined GitHub prerequisite

## Usage model

Use the plugin as the canonical and only supported skill surface for these
maintainer workflows in `git-slop`. The repo-local marketplace entry installs
it by default.

Repo-local overlays live outside the plugin:

- `AGENTS.md`: always-on repo behavior
- `.codex/README.md`: runtime map and custom-agent boundary
- `config/github/README.md`: repo-owned backlog/project overlay
- `config/labels/README.md`: repo-owned label palette overlay
