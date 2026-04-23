---
name: "merge-on-green"
description: "Use this skill when an eligible automation PR should be checked against merge gates and merged only if every gate is green."
---

# Merge On Green

Use this skill to verify merge safety for an automation PR and perform the
final merge only when every gate already passes.

## Prerequisites

- Local interactive use expects the official GitHub Codex plugin.
- Run `python3 ../../scripts/preflight_github_plugin.py` if you need to confirm the local prerequisite.

## Read First

- `../_shared/references/github-mutation-contract.md`
- `../_shared/references/workflow-tooling-surface.md`

## Workflow

1. Resolve the current repository context with `uv run agent-tools github current-repo --format json` when repository metadata is needed.
2. Inspect the target PR state with standard `gh pr view` and workflow results with `gh run view`.
3. Confirm checks, labels, branch freshness, and review state are all merge-safe.
4. Use standard `gh pr merge` only after every gate passes and keep the decision evidence in `.artifacts/merge-on-green/...`.
