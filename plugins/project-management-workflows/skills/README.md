# Reusable Project Management Skills

These skills are the canonical reusable workflow layer for backlog,
governance, planning, release, and automation work.

Use them for:

- backlog preview and apply flows
- deterministic governance checks
- quarter planning
- review and plan handoffs into backlog-ready artifacts
- dependency remediation, merge gating, docs taxonomy, and release publishing

This plugin is installed by default for `git-slop`.

## Skill Metadata

Plugin skill metadata lives in:

- `skills/*/agents/openai.yaml`

Every skill metadata file should define:

- `interface.display_name`
- `interface.short_description`
- `interface.default_prompt`
- `policy.allow_implicit_invocation`
- `dependencies.tools`

Rules:

- `default_prompt` must explicitly mention `$skill-name`
- implicit invocation stays limited to narrow preview-safe workflows
- `dependencies.tools` should declare `type: connector` and `value: github`
  whenever a skill is GitHub-touching during local interactive use
- GitHub-touching skills hard-require both the official GitHub Codex plugin and
  the bundled GitHub connector mapping; see
  `_shared/references/github-runtime-prerequisites.md`

Keep repo-wide behavior in `AGENTS.md`, runtime wiring in `.codex/README.md`,
and repo-owned overlays in `config/*/README.md`.
