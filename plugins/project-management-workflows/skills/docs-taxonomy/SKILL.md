---
name: "docs-taxonomy"
description: "Use this skill when docs, skills, plugin references, or agent guidance should be moved back into the intended taxonomy with narrow docs-only edits."
---

# Docs Taxonomy

Use this skill to detect taxonomy drift across docs, skills, plugin guidance,
and custom-agent surfaces, then prepare the smallest docs-only fix.

## Read First

- `../_shared/references/agent-decision-patterns.md`
- `../_shared/references/workflow-tooling-surface.md`

## Workflow

1. Run the repo's checked-in Codex, plugin, or runtime validator when one exists. If there is no dedicated validator, use the narrowest non-mutating docs/runtime checks available in that repo.
2. Inspect taxonomy drift across docs, plugin guidance, and agent configuration.
3. Make the smallest docs-only changes needed to restore the intended layering.

## Optional Publish

If GitHub runtime is already available and you want to publish the docs-only
change immediately, use standard `gh pr create` or `gh pr edit` and keep the
evidence in `.artifacts/docs-taxonomy/...`.
