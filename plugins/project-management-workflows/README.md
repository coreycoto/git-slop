# Project Management Workflows Plugin

This repo-local plugin packages reusable project-management workflows that can
be consumed from `git-slop` or installed home-locally for use from other
repositories.

It packages:

- reusable maintainer workflow skills
- shared references and decision aids
- lightweight preflight guidance for external prerequisites
- the GitHub connector mapping for GitHub-touching local interactive workflows
- canonical shell-first workflow guidance for deterministic `agent-tools`, `gh`,
  `git`, and artifact usage

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

The shipped plugin remains shell-first in this wave. Experimental MCP work, if
present in adjacent repos, is read-only and optional. This plugin does not
bundle `.mcp.json`, and its default workflows still assume `agent-tools`, `gh`,
and `git` as the canonical execution surface.

For CI and GitHub Actions, do not assume marketplace-installed connectors are
available. Use checked-out repo files, prompt files, custom agents, `gh`,
GitHub tokens, `agent-tools`, and repo scripts instead.

## Structure

- `.codex-plugin/plugin.json`: plugin manifest
- `.app.json`: bundled GitHub connector mapping for local interactive use
- `skills/`: reusable workflow skills
- `skills/_shared/references/`: reusable decision aids and policy references
- `scripts/manage_home_local_plugin.py`: manage a home-local marketplace entry
- `scripts/smoke_home_install.py`: temp-home install and runtime smoke harness
- `scripts/preflight_github_surface.py`: local preflight for the combined GitHub prerequisite

## Install Modes

### Repo-local inside `git-slop`

`git-slop` ships a repo-local marketplace entry in `.agents/plugins/marketplace.json`
that installs this plugin by default for work inside this repo.

### Home-local for other repos

Use the checked-out helper script to install the plugin into your home-local
Codex marketplace:

```bash
python3 plugins/project-management-workflows/scripts/manage_home_local_plugin.py install
python3 plugins/project-management-workflows/scripts/manage_home_local_plugin.py status
```

Remove it with:

```bash
python3 plugins/project-management-workflows/scripts/manage_home_local_plugin.py remove
```

Use `--home /path/to/temp-home` to target a temp Codex home during smoke tests
or clean-room validation.

## Usage model

Use the plugin as the canonical skill surface for these maintainer workflows.
Repo-specific overlays remain outside the plugin.

Repo-local overlays live outside the plugin:

- `AGENTS.md`: always-on repo behavior
- `.codex/README.md`: runtime map and custom-agent boundary
- `config/github/README.md`: repo-owned backlog/project overlay
- `config/labels/README.md`: repo-owned label palette overlay
