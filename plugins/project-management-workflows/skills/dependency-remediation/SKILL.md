---
name: "dependency-remediation"
description: "Use this skill when a narrow dependency or CVE remediation should be prepared with bounded verification and standard PR flows."
---

# Dependency Remediation

Use this skill to prepare the smallest defensible dependency or vulnerability
remediation and carry it through bounded verification and PR publication.

## Prerequisites

- Local interactive use requires both the official GitHub Codex plugin and the bundled GitHub connector mapping.
- Run `python3 ../../scripts/preflight_github_surface.py` if you need to confirm the combined local prerequisite before continuing.

## Read First

- `../_shared/references/github-runtime-prerequisites.md`
- `../_shared/references/github-mutation-contract.md`
- `../_shared/references/workflow-tooling-surface.md`

## Workflow

1. Resolve the current repository context with `uv run agent-tools github current-repo --format json` when repository metadata is needed.
2. Inspect the triggering dependency or CVE scope and apply the smallest defensible manifest or lockfile remediation.
3. Run bounded verification only for the changed dependency surface.
4. Use standard `gh pr view`, `gh pr create`, and `gh pr edit` flows for PR lifecycle and keep evidence in `.artifacts/dependency-remediation/...`.
