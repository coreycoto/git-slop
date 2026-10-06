# Label Palette Config

This directory holds the repo-owned label palette overlay for `git-slop`.

Reusable policy guidance comes from the public `coreycoto/agent-plugins`
packages pinned in `.agents/plugins/marketplace-source.json`; the portable
`git-slop` Agent Plugin documents this repository's product workflow.

Use `$project-management:project-governance` for label ownership policy and
`$gh-steward:gh-steward-reviewed-governance` for native snapshots, reviewed
label plans, exact-hash applies, and durable recovery.

The relevant reusable references are:

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
