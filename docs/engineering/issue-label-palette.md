# Issue Label Palette

- Status: current
- Audience: maintainer
- Canonical: yes

This document records the preferred backlog label palette and the deterministic
workflow for applying repo-managed label colors in GitHub.

## Goal

Keep the backlog label vocabulary small, semantically legible, and visually
distinct in the GitHub Issues and Projects UI.

GitHub default labels remain the visual baseline. Repo-managed custom labels
should complement that baseline rather than collide with it.

## Preferred Vocabulary

| Label | Owner | Semantic role | Target color |
| --- | --- | --- | --- |
| `enhancement` | GitHub default | feature work | `A2EEEF` |
| `question` | GitHub default | research or decision work | `D876E3` |
| `bug` | GitHub default | broken behavior | `D73A4A` |
| `documentation` | GitHub default | docs-primary work | `0075CA` |
| `epic` | repo-managed | multi-issue workstream | `C27C2C` |
| `maintenance` | repo-managed | upkeep, CI, DX, cleanup | `4D6575` |

## Repo-Managed Labels

Only these labels are repo-managed today:

- `epic`
- `maintenance`

Their palette choices are intentional:

- `epic` uses warm copper so roadmap umbrellas stay distinct from bug red.
- `maintenance` uses muted slate-blue so operational work stays distinct from
  docs blue and GitHub default grays.

The checked-in manifest lives at:

- `config/labels/label_palette.json`

Use the maintainer CLI to validate or preview the checked-in palette:

```bash
uv run agent-tools github sync-label-palette --check
uv run agent-tools github sync-label-palette
```

The sync path manages repo-owned labels only. It does not delete labels, and it
does not restyle GitHub defaults in v1.
