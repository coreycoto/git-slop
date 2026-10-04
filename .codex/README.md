# Codex Runtime

This directory defines the repo-local Codex runtime surface for `git-slop`:

- `AGENTS.md`: always-on repository policy
- `config.toml`, `ci_*.config.toml`, and `rules/`: local profiles and approval
  boundaries
- `agents/*.toml`: narrow execution roles
- `.agents/plugins/marketplace-source.json`: immutable source pin for the
  public `coreycoto/agent-plugins` project-management and product-development
  Agent Plugins
- `.agents/gh-steward.lock.json`: exact native `gh-steward` source revision and
  four-platform release checksums
- `plugins/git-slop`: portable Git Slop product instructions
- `.github/codex/prompts/*` and `.github/codex/schemas/*`: contracts for Codex
  workflows
- `xtask/`: repository-owned validation for Codex, workflows, and release
  contracts

`scripts/prepare-codex-plugins.sh` installs the pinned Codex CLI and selected
plugins into an isolated `CODEX_HOME` under `RUNNER_TEMP`.
`scripts/with-gh-steward.sh` acquires and verifies the matching attested native tool release
without changing the runner's global extension installation. The release
helper and acquisition receipt bind the binary to its exact source commit,
version, platform, checksums, and GitHub provenance. Unqualified lock values
are intentionally rejected until the final clean source and four release
digests are supplied.

Reusable project and product development workflow guidance comes from the
public Agent Plugins source. GitHub repository operations use `gh` and the
native `gh-steward` extension. Its reviewed plans bind exact repository,
Project, source inventory, and before-state; `--approve-plan-sha` identifies a
plan but does not grant permission. After an interrupted operation, restore
the exact plan and durable journal. If evidence has expired or cannot be
matched to the target and run, stop with `recovery_needed` instead of preparing
a replacement plan.

## Approval and Publication

Interactive local sessions default to `approval_policy = "on-request"`.
Non-interactive CI profiles use `approval_policy = "never"` and rely on the
workflow's explicit permission scope and checked-in policy. Privileged
`pull_request_target` workflows use the trusted base checkout, do not persist
checkout credentials, and keep GitHub tokens on the steps that need them.

The execution-state workflow uses a target-specific concurrency group without
cancel-in-progress. Before a new plan, it inspects the complete workflow and
artifact history, restores the latest pending plan and journal, and fails
closed when the previous write's outcome cannot be proven. Its artifacts keep
the reviewed plan, apply receipt, typed journal, source target, and run identity
for future-process recovery.

Prefer `git push`, `gh release`, and `gh pr merge`; prompt before those commands
in interactive sessions. Do not use the GitHub Git Data API to publish unless
the user explicitly requests that fallback.

## Custom Agents

Custom agents should stay narrow, match the workflow prompt that invokes them,
and reference plugin-owned skills instead of copying reusable policy. The
current workflow roles are `dependency_patcher`, `merge_gatekeeper`, and
`governance_auditor`; `docs_taxonomist` and `release_publisher` support their
separate on-demand and release workflows.

The portable `git-slop` Agent Plugin's root manifest is authoritative under Agent
Plugins 1.0.0. Optional Codex interface metadata lives under
`extensions.com.openai`; the compatibility metadata mirror remains minimal.
Keep reusable project governance in the public `project-management` plugin,
product refactoring in `product-development`, and Git Slop product guidance in
`plugins/git-slop`.

Run `cargo xtask validate-codex` after changing this surface and
`cargo xtask validate-workflows` after changing workflow wiring. Use
`--require-codex-cli` only when the validation environment is expected to have
the Codex CLI installed.
