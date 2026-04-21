# Agent Skill Metadata

- Status: current
- Audience: maintainer
- Canonical: yes

This document defines the repo-local `agents/openai.yaml` contract for
`.agents/skills/`, the shared family taxonomy, and the rules for implicit
invocation.

## Deterministic Source Of Truth

Treat the metadata surface as generated state.

- machine-readable manifest:
  - `config/agents/skill_metadata_manifest.json`
- sync and drift check:
  - `uv run agent-tools skills sync-openai-metadata --repo-root .`
  - `uv run agent-tools skills sync-openai-metadata --repo-root . --check`
- shared family icon sources:
  - `.agents/skills/_shared/assets/skill-icons/`

Edit the manifest or the shared icon sources first, then rerun the sync tool.
Do not hand-edit generated `agents/openai.yaml`.

## Metadata Contract

Every repo-local skill should expose the same metadata surface:

- `interface.display_name`
- `interface.short_description`
- `interface.icon_small`
- `interface.icon_large`
- `interface.brand_color`
- `interface.default_prompt`
- `policy.allow_implicit_invocation`

Rules:

- `default_prompt` is a single sentence and must explicitly mention the skill as `$skill-name`
- both icon fields should point at `./assets/icon.svg`
- `brand_color` comes from the skill family, not per-skill improvisation

## Family Taxonomy

`git-slop` uses these families:

| Family | Brand Color | Current Skills |
| --- | --- | --- |
| `intake` | `#0F766E` | `intake-preview`, `intake` |
| `review` | `#4338CA` | `review-to-backlog-preview`, `review-to-backlog-apply` |
| `governance` | `#334155` | `github-backlog-mutate`, `label-palette-design` |
| `planning` | `#A16207` | `ensure-quarter-milestones`, `plan-quarter-preview`, `plan-quarter-apply` |

## Invocation Policy

Implicit invocation is allowed only when all of the following are true:

- the skill is non-mutating
- the trigger boundary is narrow and easy to infer
- the workflow does not hide GitHub writes or queue-driving behavior

Current implicit-enabled skills:

- `intake-preview`
- `review-to-backlog-preview`
- `plan-quarter-preview`
- `label-palette-design`

Everything else remains explicit-only.
