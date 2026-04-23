# Repository Skills

This directory is the canonical reusable workflow surface for `git-slop`.

Use `AGENTS.md` for always-on repo policy. Use `docs/engineering/` for durable
human-facing policy. Use `src/agent_tools/` for shared deterministic logic, and
keep per-skill orchestration in the local `scripts/run.py` entrypoints.

Current skill families:

- `intake`: normalize repo-local markdown and DOCX research into backlog-ready artifacts
- `review`: turn deterministic review findings into backlog deltas
- `governance`: validate labels, issue mutation plans, and backlog structure
- `planning`: enforce quarter milestone policy and preview planning handoffs into backlog-ready maintenance work

Keep generated metadata in sync with:

```bash
uv run agent-tools agents sync-skill-openai-metadata
uv run agent-tools agents sync-skill-openai-metadata --check
```

Current repo-local skills:

- `intake-preview`
- `intake`
- `review-to-backlog-preview`
- `review-to-backlog-apply`
- `ensure-quarter-milestones`
- `plan-quarter-preview`
- `plan-to-backlog-preview`
- `plan-quarter-apply`
- `github-backlog-mutate`
- `label-palette-design`
