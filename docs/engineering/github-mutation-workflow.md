# GitHub Mutation Workflow

- Status: current
- Audience: contributor
- Canonical: yes

This document defines the safe default workflow for GitHub-only mutations in
this repository.

## Preflight Before Mutation

Before any GitHub write:

- verify the current local branch and `HEAD` SHA
- verify whether the tracked worktree is clean or dirty
- verify the target repository and the planned mutation surface
- verify GitHub auth in the current shell

Preferred checks:

```bash
gh auth status
uv run agent-tools github current-repo --format text
uv run agent-tools github sync-project-config
```

Treat prior assistant claims about projects, issues, milestones, labels, or
field values as untrusted until a current-turn GitHub read confirms them.

## GitHub-Only Closeout

When the work is GitHub-only and no repo-tracked files changed:

- do not use code-change verification as the closeout gate
- confirm the live GitHub result directly instead
- report the exact `gh` or `agent-tools github` commands that confirmed the final state

## Scope Rules

Use automation for:

- project snapshot generation
- issue graph generation
- label palette preview and apply
- milestone policy checks
- review-to-backlog previews and controlled apply flows
- quarter-plan preview and controlled apply flows

Do not silently mutate:

- issue bodies unless the current command is explicitly an apply path
- parent/sub-issue links outside a reviewed backlog mutation
- milestone assignments outside a reviewed quarter-plan apply path
- project `Priority` or `Queue Order` as part of governance auto-fix

## Manual UI Steps

If a required GitHub step is UI-only, leave it explicitly manual and
outstanding rather than implying API or CLI confirmation.

## One Mutation Plan At A Time

When a workflow is applying GitHub changes:

- generate artifacts first
- validate the reviewed delta
- apply one reviewed delta at a time
- verify the live GitHub result immediately after

GitHub Projects note:

- bulk project-item inserts can return temporary GraphQL conflict errors such as `add_000`
- if that happens, retry serially or in very small batches
- treat those conflicts as transient coordination failures, not as evidence that the target issue is invalid

Artifact-first mutation paths should write under:

- `.artifacts/github-governance/<timestamp>/`
- `.artifacts/review-to-backlog/<timestamp>/`
- `.artifacts/quarter-plan/<timestamp>/`
