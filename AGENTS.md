# Repository Agent Policy

`git-slop` uses these guidance layers:

- `AGENTS.md`: always-on repo-wide policy and execution constraints
- `.codex/README.md`: Codex runtime map for config, rules, agents, prompts, and schemas
- `.agents/plugins/marketplace-source.json`: pinned marketplace source and prebuilt-runtime integrity manifest
- `.agents/plugins/marketplace.json`: local publication manifest for the `git-slop` Codex plugin
- installed `project-management-workflows` plugin from `coreycoto/agent-plugins`: reusable workflow contract and metadata
- local `git-slop` plugin under `plugins/git-slop`: product-specific usage, install, report, interpretation, planning, and adoption guidance
- private standalone Rust `xtask/` workspace: repo-owned Codex, workflow, repository, distribution, and release validation
- `config/github/README.md`: repo-owned backlog/project overlay
- `config/labels/README.md`: repo-owned label palette overlay

## Publication Rules

- Prefer standard `git push`, `gh release`, and `gh pr merge`.
- If direct publication is blocked by runtime policy, stop and report it.
- Do not publish commits, branches, tags, or releases through the GitHub Git Data API unless the user explicitly asks for that fallback.

## Workflow Boundaries

- Keep the public `git slop` CLI focused on detector, report, explain, and plan behavior.
- Keep reusable maintainer workflow instructions in the installed project-management plugin from `coreycoto/agent-plugins`.
- Keep the local `git-slop` Codex plugin focused on product-specific CLI usage and consumer adoption guidance.
- Keep repo-owned maintainer contract validation in the private standalone Rust `xtask/` workspace and validate it with its committed lockfile.
- Keep only the pinned marketplace-source manifest and workflow invocation in this consumer repo; invoke the publisher-owned prebuilt runtime only through `scripts/with-agent-plugins.sh`.
- Keep `agent_plugins` behavior tests, bootstrap implementation, and clean-room consumer smoke in `coreycoto/agent-plugins`.
- Keep repo-specific overlays next to the repo-owned data they describe under `config/*/README.md`.
- Keep custom agents thin: they should reference plugin skills and only add role, sandbox, model, and delegation guidance.

## Automation Rules

- Use prompt files under `.github/codex/prompts/` for every Codex-powered workflow job.
- Use schema files under `.github/codex/schemas/` when a workflow expects structured output before applying a mutation.
- Treat the official GitHub Codex plugin as a local interactive prerequisite, not as a CI dependency.
- In CI, validate repo-owned contracts with `cargo xtask`; rely on checked-out repo files, prompt files, custom agents, `gh`, and GitHub tokens.
- When a workflow needs the external `agent_plugins` runtime, acquire it into an ephemeral per-job directory with `scripts/with-agent-plugins.sh --prepare`, then verify it separately with `--verify` before using its direct CLI.
- Scope `AGENT_PLUGINS_READ_TOKEN` to the dedicated prepare step. Never expose it to runtime execution, persist it in Git configuration, or use it from pull-request-controlled code.
- Treat the manifest-pinned source revision and archive digest as consumer-owned integrity checks. Reject an unsafe archive or any release metadata, embedded revision, target, or digest mismatch.
- Use `marketplace`, `github project-snapshot`, and `github execution-state` as the canonical runtime commands. Interpreter mode is confined to the wrapper for isolated publisher identity verification and its legacy compatibility entry point.
- Do not use Actions caching, install an interpreter or toolchain for this runtime, or perform a project dependency sync. The verified SCIE and its embedded marketplace must need no further publisher acquisition after preparation.
- In execution-state sync, keep the project PAT off job scope and pass it as `GH_TOKEN` only to the direct project operations. In privileged `pull_request_target` automation, likewise pass the repository mutation token only to the deliberate Codex mutation step. Acquisition, verification, and publisher identity smoke must not inherit either credential.
- For privileged `pull_request_target` jobs, validate and snapshot trusted base Codex config, agents, prompts, and schemas before checking out the requested head. Do not execute head-owned maintainer tooling or persist checkout credentials; expose `github.token` only on the deliberate mutation step.
- Keep the public release workflow independent of private `agent-plugins` credentials and runtime acquisition.
