# Repository Agent Policy

`git-slop` uses these guidance layers:

- `AGENTS.md`: always-on repo-wide policy and execution constraints
- `.codex/README.md`: Codex runtime map for config, rules, agents, prompts, and schemas
- `.agents/plugins/marketplace-source.json`: pinned public project-management and product-development plugin source
- `.agents/gh-steward.lock.json`: exact native GitHub tool source and release-asset integrity pins
- `.agents/plugins/marketplace.json`: local Codex marketplace for the portable `git-slop` Agent Plugin
- public Agent Plugins from `coreycoto/agent-plugins`: reusable project and product-development guidance
- `coreycoto/gh-steward`: native GitHub CLI extension and focused reviewed-operation skills
- local `git-slop` Agent Plugin under `plugins/git-slop`: portable product-specific usage, install, report, interpretation, planning, and adoption guidance
- private standalone Rust `xtask/` workspace: repo-owned Codex, workflow, repository, distribution, and release validation
- `config/github/README.md`: repo-owned backlog/project overlay
- `config/labels/README.md`: repo-owned label palette overlay

## Publication Rules

- Prefer standard `git push`, `gh release`, and `gh pr merge`.
- Assemble and verify every release asset on a draft. Every future stable public
  release must be GitHub-immutable before any post-publication package-manager
  dispatch; recovery must fail closed when `.immutable` is not `true`.
- If direct publication is blocked by runtime policy, stop and report it.
- Do not publish commits, branches, tags, or releases through the GitHub Git Data API unless the user explicitly asks for that fallback.

## Workflow Boundaries

- Keep the public `git slop` CLI focused on detector, report, explain, and plan behavior.
- Keep reusable project and product-development instructions in the public Agent Plugins packages. Keep GitHub state reads, reviewed plans, applies and durable receipts in the native `gh-steward` extension.
- Keep the local `git-slop` Agent Plugin focused on portable product-specific CLI usage and consumer adoption guidance.
- Keep `plugins/git-slop/plugin.json` authoritative. Until a shipped Codex app-server resolves its complete metadata, retain `.codex-plugin/plugin.json` only as an exact metadata-only compatibility mirror with no skills, MCP, app, or hook declarations.
- Keep repo-owned maintainer contract validation in the private standalone Rust `xtask/` workspace and validate it with its committed lockfile.
- Keep the public plugin source and `gh-steward` release locks consumer-owned. Acquire the native extension only through `scripts/with-gh-steward.sh`; install the public plugins into an isolated Codex home only through `scripts/prepare-codex-plugins.sh`.
- Do not use the private Agent Development repository as a runtime or skills source in this consumer.
- Keep repo-specific overlays next to the repo-owned data they describe under `config/*/README.md`.
- Keep custom agents thin: they should reference plugin skills and only add role, sandbox, model, and delegation guidance.

## Product Surface Discipline

- Maximize demonstrated user value while minimizing permanent surface area.
- Keep product and maintainer context in this monorepo. Prefer explicit logical
  boundaries, internal modules, and reversible experiments over additional
  repositories or public contracts.
- Treat every new command, option, configuration key, machine schema, output
  format, Action input or output, Agent Plugin skill, workflow, integration,
  generated artifact, and compatibility promise as long-lived surface until
  proven otherwise.
- Give every contract one state: `internal`, `experimental`, `candidate`, or
  `stable`. Promotion must follow demonstrated use; implementation completeness
  alone is not evidence that users need a stable contract.
- Before admitting or promoting a contract, identify its consumer and job,
  direct evidence of need, why an existing contract is insufficient, the
  smallest reversible experiment, permanent maintenance cost, expiry, and
  deletion conditions. Prefer consolidation, an internal seam, or deferral when
  those answers are weak.
- Every pull request must include a multidimensional surface-area ledger. Record
  added, removed, or consolidated surface and its contract state; do not reduce
  the ledger to a composite score. Automated path dimensions are advisory
  evidence and never a default gate.
- Resolve audit findings explicitly as `implement`, `consolidate`, `defer`,
  `accept`, or `won't fix`. Closing every finding is not the objective, and a
  green closure metric is not proof of user value.

## Automation Rules

- Use prompt files under `.github/codex/prompts/` for every Codex-powered workflow job.
- Use schema files under `.github/codex/schemas/` when a workflow expects structured output before applying a mutation.
- Treat the official GitHub Codex plugin as a local interactive prerequisite, not as a CI dependency.
- In CI, validate repo-owned contracts with `cargo xtask`; rely on checked-out repo files, prompt files, custom agents, `gh`, and GitHub tokens.
- Acquire `gh-steward` from its immutable source revision and attested, checksum-pinned release asset into `RUNNER_TEMP`; fail closed while any release pin is unqualified. Never install the workflow binary globally.
- Use the step-scoped GitHub job token for release attestation reads. Scrub credentials from public Git fetches and offline binary checks; keep offline receipt verification and plugin installation token-free.
- Install Codex and the selected public Agent Plugins only into a `RUNNER_TEMP`-scoped `CODEX_HOME`; do not change global plugin state.
- Execution State Sync uses manual default-branch preparation and a separate
  reviewed-run/exact-hash apply dispatch. In execution-state sync, keep the project PAT off job scope and pass it as `GH_TOKEN` only to the exact snapshot, prepare and apply steps that need it. In privileged `pull_request_target` automation, expose the repository mutation token only to the deliberate Codex mutation step.
- Preserve the exact execution plan and durable `gh-steward` journal as a run-scoped artifact. Before any new plan, reconcile the complete run/artifact history and restore the exact pending plan plus journal. If the exact target, run, plan hash and journal cannot be proven, record `recovery_needed` and stop; never create a replacement plan past an unknown write. Keep target concurrency `cancel-in-progress: false`.
- For privileged `pull_request_target` jobs, validate and snapshot trusted base Codex config, agents, prompts, and schemas before checking out the requested head. Do not execute head-owned maintainer tooling or persist checkout credentials; expose `github.token` only on the deliberate mutation step.
- Keep public package and Action release workflows independent of consumer plugin/tool acquisition and consumer-only credentials.
