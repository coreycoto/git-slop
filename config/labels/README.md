# Label Palette Config

This directory holds the repo-owned label palette overlay for `git-slop`.

Use the reusable workflow contract from the plugin for sync and mutation rules:

- `plugins/project-management-workflows/skills/_shared/references/label-palette-contract.md`
- `plugins/project-management-workflows/skills/_shared/references/github-mutation-contract.md`
- `plugins/project-management-workflows/skills/_shared/references/workflow-tooling-surface.md`

## Files

- `label_palette.json`: canonical checked-in label vocabulary, ownership, and target colors

## Local Overlay

Preferred label vocabulary for this repo:

- `enhancement`
- `question`
- `bug`
- `documentation`
- `epic`
- `maintenance`

Repo-managed labels today:

- `epic`
- `maintenance`

Default taxonomy mapping:

- `Enhancement:` -> `enhancement`
- `Research:` -> `question`
- `Bug:` -> `bug`
- `Epic:` -> `epic`
- `Maintenance:` -> `maintenance`
