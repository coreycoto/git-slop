# Codex Runtime

This directory defines the repo-local Codex runtime surface for `git-slop`.

Use the runtime layers like this:

- `AGENTS.md`: always-on repo-global execution rules
- `.codex/config.toml`: project-scoped defaults and app permissions
- `.codex/ci_*.config.toml`: standalone non-interactive profiles loaded by `--profile`
- `.codex/rules/*.rules`: interactive approval prompts for sensitive shell commands
- `.codex/agents/*.toml`: custom execution roles
- `.agents/plugins/marketplace-source.json`: pinned marketplace source manifest
- `.agents/plugins/marketplace.json`: local manifest for the `git-slop` Codex plugin
- installed `project-management-workflows` plugin from `coreycoto/agent-plugins`: canonical reusable workflow contract
- `plugins/git-slop`: product-owned plugin for installing, running, interpreting, and adopting `git-slop`
- `.github/codex/prompts/*`: workflow prompts that explicitly name the custom agents to use
- `.github/codex/schemas/*`: structured-output schemas for Codex-driven workflows
- `xtask/`: private standalone Rust validation and release automation for repo-owned contracts
- `scripts/with-agent-plugins.sh`: isolated launcher for the manifest-pinned prebuilt runtime

The consumer-owned boundary is the immutable marketplace-source manifest and
workflow invocation. The manifest pins both source revision and release archive
digest; the wrapper verifies release metadata, target, archive safety, and the
SCIE's embedded revision. The `agent-plugins` publisher owns marketplace
bootstrap implementation, reusable runtime behavior tests, and clean-room
consumer smoke. Repo-owned validation stays in the private standalone Rust
`xtask/` workspace; this repository does not carry a parallel maintainer
runtime.

## Approval And Publication

Interactive local sessions should default to `approval_policy = "on-request"`.
Non-interactive CI profiles should use `approval_policy = "never"` and rely on
explicit workflow permissions instead of fresh approvals.

Codex profiles are standalone files named `<profile>.config.toml`. Workflows
copy both the base config and these profile files into their isolated
`CODEX_HOME`, then install the pinned, embedded marketplace through the direct
`marketplace` CLI exposed by `scripts/with-agent-plugins.sh`. Each job prepares
the SCIE under `RUNNER_TEMP` with the read token scoped to that step, verifies it
without the token, and performs no further publisher acquisition afterward. The
embedded marketplace installs offline; GitHub commands retain the workflow's
GitHub token for their intended API calls. Do not add Actions caching, project
dependency synchronization, consumer language-runtime setup, or legacy
`[profiles.<name>]` tables to `.codex/config.toml`. Workflow calls use the
direct CLI; interpreter mode is confined to isolated identity verification and
the legacy compatibility entry point.

Execution-state sync uses `pull_request_target` so the workflow definition and
runtime launcher both come from the trusted base. Active PR events pin the
payload's base SHA; closed events use the event's current base SHA so merge
processing uses the launcher that is actually on the base branch. The workflow
writes a non-secret run-context diagnostic before runtime acquisition, making
early failures uploadable. It scopes its project token to its two direct GitHub
operations, so publisher verification and interpreter smoke do not inherit the
PAT. Fork pull requests that cannot safely receive the acquisition secret skip
the private-runtime job.

Privileged `pull_request_target` workflows validate the trusted base and
snapshot its Codex config, profiles, agents, prompt, and schema under
`RUNNER_TEMP` before checking out the requested head. They do not execute
head-owned maintainer tooling, use head-owned Codex inputs, or persist checkout
credentials. The repository token is supplied only to the deliberate Codex
mutation step. The public release workflow never acquires or invokes this
private runtime.

Publication rules:

- prefer `git push`, `gh release`, and `gh pr merge`
- prompt before those commands in interactive sessions
- do not fall back to GitHub Git Data API publication unless the user explicitly requests it

## Custom Agent Boundary

Custom agents are specialized workers, not always-on policy.

They should:

- stay narrow and opinionated
- match the workflow prompts that explicitly ask Codex to use them
- reference plugin-owned skills for workflow contract
- keep only role, sandbox, model, and delegation guidance
- avoid becoming the primary store for reusable workflow or repo policy

The `git-slop` Codex plugin is the canonical reusable guidance surface for the
product CLI itself. Keep generic backlog, governance, and release workflows in
the installed `project-management-workflows` plugin.

Current project-scoped agents:

- `dependency_patcher`
- `merge_gatekeeper`
- `governance_auditor`
- `docs_taxonomist`
- `release_publisher`

## Workflow Assets

Codex-driven GitHub Actions should keep their task contract in checked-in
prompt and schema files. The schema files are workflow-owned assets. They are
not auto-discovered by Codex or by custom agents; workflows must pass them
explicitly via `--output-schema`.

Run `cargo xtask validate-codex` after changing this surface and
`cargo xtask validate-workflows` after changing workflow wiring. Use
`--require-codex-cli` only when the validation environment is expected to have
the Codex CLI installed.
