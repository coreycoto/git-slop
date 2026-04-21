# Agent Surface

This directory is the canonical repo-local agent surface for `git-slop`.

Use the layers like this:

- `AGENTS.md`: always-on repo policy and execution constraints
- `.agents/skills/README.md`: repo-local workflow catalog
- `docs/engineering/agent-guidance-governance.md`: placement rules for agent docs, notes, and follow-up work
- `docs/engineering/agent-skill-metadata.md`: generated `agents/openai.yaml` contract and sync workflow
- `docs/engineering/github-mutation-workflow.md`: manual-first GitHub mutation policy
- `docs/engineering/backlog-governance.md`: backlog shape, title prefixes, project fields, and milestone policy
- `src/agent_tools/`: shared deterministic logic behind repo-local skills

Use `agent-tools` for maintainer-only automation. Keep the public `git-slop`
CLI focused on detector behavior.
