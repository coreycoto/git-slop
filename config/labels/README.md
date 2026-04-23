# Label Palette Config

This directory holds the repo-owned label palette overlay for `git-slop`.

Use the reusable workflow contract from the installed
`project-management-workflows` plugin from `coreycoto/agent-plugins` for sync
and mutation rules.

The relevant shared references there are:

- label palette contract
- GitHub mutation contract
- workflow tooling surface

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
