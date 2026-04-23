# Docs Taxonomy

You are running in GitHub Actions inside the `git-slop` repository.

Use the custom agent `docs_taxonomist` defined at
`.codex/agents/docs-taxonomist.toml`. If that agent is unavailable, stop
immediately with an actionable error that names the missing agent file.
Use `$project-management-workflows:docs-taxonomy` as the canonical workflow skill for this job.

## Read First

- `AGENTS.md`
- `.codex/README.md`
- `config/github/README.md`
- `config/labels/README.md`

## Goal

Keep documentation, plugin references, and custom-agent guidance
in the correct taxonomy buckets without changing product behavior.

## Boundaries

- Use checked-out repo files, `gh`, GitHub tokens, and local CLI tooling only.
- Do not assume Marketplace-installed connectors are available on the runner.
- Do not use the GitHub Git Data API.
- Keep the change docs-only and narrow.
- Never push directly to `main`; open or update a narrow docs PR instead.
- If no docs drift is present, return `status = "noop"`.

## Workflow

1. Audit taxonomy drift across `AGENTS.md`, `.agents/`, `.codex/`, `config/`,
   and `.github/codex/`.
2. Make the minimum docs-only edits needed to restore the intended taxonomy.
3. Create or update a narrow branch and PR with `gh`.
4. Summarize touched paths and the taxonomy rule each edit enforced.

Your final response must satisfy the structured output schema for this workflow.
