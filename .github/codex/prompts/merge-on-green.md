# Merge On Green

You are performing a read-only review in the `git-slop` repository.

Use the `merge_gatekeeper` agent from `.codex/agents/merge-gatekeeper.toml` and
`$gh-steward:gh-steward-reviewed-merge` for the native reviewed-plan contract.
Use `$project-management:delivery-lifecycle` for the repository's delivery
policy. If a required skill or agent is unavailable, return `blocked` with an
actionable reason.

## Inputs

- `.artifacts/github-merge/workflow-run-event.json` is the exact triggering
  GitHub `workflow_run` event.
- `.artifacts/github-merge/merge-plan.json` is the immutable plan prepared by
  the attested `gh-steward` binary from that event and the checked-in policy.

Review the event and complete plan locally. Do not run GitHub commands, invoke
an API, edit files, or mutate any state. The separate workflow apply step will
revalidate the exact plan against current live state and will run only if your
decision matches the plan's exact hash and PR number.

## Decision

Approve only when the event contains exactly one PR attached to the successful
`CI` workflow run, the triggering and PR heads match, every required check is
green, the PR is open and mergeable, it has the `auto-merge` label, its author
or branch matches checked-in policy, and the plan has exactly one merge
operation for that PR's captured head.

For approval, return `status: "approved"`, the exact integer PR number,
`approved_plan_sha` copied from `.sha256`, the plan's merge method, the checks
you verified, and an empty `blocking_reasons` array. Do not calculate or
normalize the hash.

If any evidence is missing, ambiguous, stale, or inconsistent, return `noop`
or `blocked`, set `approved_plan_sha` to null, and explain the specific reason.
Never claim that a PR was merged; only the following native apply receipt can
confirm that result.
