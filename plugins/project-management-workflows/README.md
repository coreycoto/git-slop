# Project Management Workflows Plugin

This repo-local plugin is the `git-slop` proving ground for reusable
project-management workflows.

It packages:

- reusable maintainer workflow skills
- shared references and decision aids
- lightweight preflight guidance for external prerequisites
- canonical workflow guidance for deterministic `agent-tools`, `gh`, and artifact usage

It does **not** bundle:

- the official GitHub Codex plugin
- a custom app mapping
- a custom MCP server

## Prerequisites

For local interactive use, install and enable the official GitHub Codex plugin.
This plugin assumes that GitHub repository, issue, PR, and project context can
be resolved through that plugin when a reusable skill needs live GitHub reads or
writes.

For CI and GitHub Actions, do not assume marketplace-installed connectors are
available. Use checked-out repo files, prompt files, custom agents, `gh`,
GitHub tokens, `agent-tools`, and repo scripts instead.

## Structure

- `.codex-plugin/plugin.json`: plugin manifest
- `skills/`: reusable workflow skills
- `skills/_shared/references/`: reusable decision aids and policy references
- `scripts/preflight_github_plugin.py`: local preflight for the GitHub plugin prerequisite

## Usage model

Use the plugin as the canonical and only supported skill surface for these
maintainer workflows in `git-slop`. The repo-local marketplace entry installs
it by default.

Repo-local overlays live outside the plugin:

- `AGENTS.md`: always-on repo behavior
- `.codex/README.md`: runtime map and custom-agent boundary
- `config/github/README.md`: repo-owned backlog/project overlay
- `config/labels/README.md`: repo-owned label palette overlay
