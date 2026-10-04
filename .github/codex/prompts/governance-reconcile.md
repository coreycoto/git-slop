# Governance Reconcile

You are reviewing a captured, read-only governance plan in the `git-slop` repository. The workflow's Codex job has no GitHub mutation token. A separate trusted job may apply only an exact plan that you approve and only within the checked-in label policy.

Use the `governance_auditor` agent from `.codex/agents/governance-auditor.toml`, `$project-management:project-governance`, and `$gh-steward:gh-steward-reviewed-governance`. If an agent or skill is unavailable, return `failed` with an actionable note.

Read `AGENTS.md`, `.codex/README.md`, `config/labels/README.md`, the captured backlog preview, and `.artifacts` package files supplied in the workspace. The only candidate for apply is `.artifacts/github-governance/label-plan.json`, whose complete source snapshot, repository identity, before-values, operations, and `sha256` must all be inspected. It is an unregistered candidate; the trusted workflow records it for apply only after validating your exact approval.

The policy boundary is the checked-in complete label palette. Only `label-create` and `label-update` operations are eligible. Verify every operation corresponds to a preferred palette entry and that the exact operation list has no extra or out-of-policy mutations. Do not approve empty plans. Do not approve issue, milestone, Project, relationship, or execution-state writes. No milestone target dates or descriptions are supplied here, so quarter milestone work must remain a preview and require authored policy.

Return `status: approved` and one `approved_plans` item only when the exact plan passes those checks. Copy its `sha256` into `plan_sha256`, and preserve operation IDs in exact plan order. If you decline the proposal after review, return `declined` or `blocked` with no approved plans and clear reasons. Return `noop` only when no mutation is warranted and `failed` when required evidence cannot be inspected. The plan hash identifies the artifact; it does not create authorization. Do not call GitHub or any mutation command, edit files, or claim that a mutation has happened.
