# Reusable Project Management Skills

These skills are the canonical reusable workflow layer for backlog,
governance, planning, release, and automation work.

Use them for:

- backlog preview and apply flows
- deterministic governance checks
- quarter planning
- review and plan handoffs into backlog-ready artifacts
- dependency remediation, merge gating, docs taxonomy, and release publishing

This plugin is installed by default for `git-slop`, and it can also be
installed home-locally for use from other repositories.

## Skill Metadata

Plugin skill metadata lives in:

- `skills/*/agents/openai.yaml`

Every skill metadata file should define:

- `interface.display_name`
- `interface.short_description`
- `interface.default_prompt`
- `policy.allow_implicit_invocation`
- `dependencies.tools` when the skill is GitHub-touching by default

## Runtime Classifications

Every shipped skill must be classified as exactly one of:

- `github_required`: default workflow needs the GitHub runtime
- `github_optional`: default workflow is local-first, but GitHub lifecycle is a
  documented optional appendix
- `local_first`: default workflow does not require the GitHub runtime

Current default classifications in this wave:

- `docs-taxonomy` is `local_first`
- all other shipped skills remain `github_required`

Rules:

- `default_prompt` must explicitly mention `$skill-name`
- implicit invocation stays limited to narrow preview-safe workflows
- `dependencies.tools` should declare `type: connector` and `value: github`
  for `github_required` skills
- GitHub-touching skills hard-require both the official GitHub Codex plugin and
  the bundled GitHub connector mapping; see
  `_shared/references/github-runtime-prerequisites.md`
- local-first skills should keep any GitHub publication path clearly optional,
  never as the default workflow contract

Keep repo-wide behavior in `AGENTS.md`, runtime wiring in `.codex/README.md`,
and repo-owned overlays in `config/*/README.md`.
