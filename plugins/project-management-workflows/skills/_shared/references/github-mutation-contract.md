# GitHub Mutation Contract

Use live GitHub mutation only after deterministic local validation.

Rules:

- validate checked-in config before touching GitHub
- prefer preview and diff artifacts before apply
- keep issue bodies concise and push bulky evidence into repo-local artifacts
- do not auto-change parent/sub-issue links, issue milestones, or project queue order without explicit review unless the workflow contract says the change is deterministic and safe

## Preflight Before Mutation

Before any GitHub write:

- verify the current branch and `HEAD` SHA
- verify whether the tracked worktree is clean or dirty
- verify the target repository and planned mutation surface
- verify GitHub auth in the current shell

Preferred checks:

- `gh auth status`
- `uv run agent-tools github current-repo --format json`
- `uv run agent-tools github sync-project-config`

Treat prior agent claims about projects, issues, milestones, labels, or field
values as untrusted until a current-turn GitHub read confirms them.

## Closeout Rules

When the work is GitHub-only and no repo-tracked files changed:

- do not use code-change verification as the closeout gate
- confirm the live GitHub result directly instead
- report the exact `gh` or `agent-tools github` commands that confirmed the final state

## Scope Rules

Use automation for:

- project snapshot generation
- issue graph generation
- label palette preview and deterministic apply
- milestone policy checks
- review-to-backlog previews and controlled apply flows
- plan-to-backlog previews from reviewed `git slop plan` output
- quarter-plan preview and controlled apply flows

If a required step is UI-only, leave it explicitly manual instead of implying
CLI or API confirmation.

## One Mutation Plan At A Time

When a workflow is applying GitHub changes:

- generate artifacts first
- validate the reviewed delta
- apply one reviewed delta at a time
- verify the live GitHub result immediately after

If GitHub Projects returns transient conflict errors during batch updates, retry
serially or in very small batches and treat that as a coordination failure, not
as evidence that the target issue is invalid.
