# Agent and Tool Surface

This directory carries the consumer-owned locks for two independent public
tooling sources:

- `.agents/plugins/marketplace-source.json` pins the public
  `coreycoto/agent-plugins` source revision and the selected
  `project-management` and `product-development` plugins.
- `.agents/gh-steward.lock.json` pins the `coreycoto/gh-steward` source commit
  and checksums for its four qualified Darwin/Linux assets.

The portable `git-slop` Agent Plugin is maintained separately in
`plugins/git-slop` and distributed through this repository's local Codex marketplace manifest at
`.agents/plugins/marketplace.json`.

`scripts/prepare-codex-plugins.sh` installs Codex CLI 0.160.0 and the three
selected plugins into an isolated `CODEX_HOME` under `RUNNER_TEMP`. It uses
immutable source revisions and does not change a runner's global plugin state.
The gh-steward source pin comes from its release lock.

`scripts/with-gh-steward.sh --prepare` acquires the exact native release into
`RUNNER_TEMP` using the source repository's release helper. The helper checks
the release tag, source commit, checksums, attestation, binary version, target,
and expected digest. `--verify` rechecks the machine-readable acquisition
receipt and staged binary before any native workflow command runs. Hosted
callers invoke the quoted `$GH_STEWARD_BIN` path directly. A binary on `PATH`
does not register a `gh` extension; acquisition does not change the host
extension installation. The lock pins GH Steward 0.6.1 at source
`5ae11ada98688e2b85ef3342b208d56c9346b32c`. Acquisition requires matching
published assets and source attestations before the consumer can use the tool.

Merge On Green adoption uses one reviewed `history_start` in the recovery
policy instead of metadata releases, promotion comments or a repository variable.
Old runs remain unknown and cannot be replayed; new interrupted operations retain
ordinary native plans, journals and verified run artifacts.

Issue [#173](https://github.com/coreycoto/git-slop/issues/173) stays open until
hosted native recovery is qualified. Review the boundary after accounting for
unfinished old operations; do not advance it to bypass interrupted native work.

Use `gh steward` for complete live snapshots and reviewed, exact-hash GitHub
plans. A plan hash identifies the saved artifact; it does not itself grant
permission. Keep operation journals and receipts after interruption. Do not
rebuild a plan or replay an ambiguous operation when its saved plan and journal
cannot be restored.

Pending native plans require `runs qualify-prepared` before package upload. It
binds the exact current workflow source and positively skipped mutation steps
to the saved plan, with no journal or apply result for that plan. Qualification
failure still preserves the package. A prepared-work checkpoint retains an open
plan across interruptions; only actual terminal receipts establish settlement.
These source proofs cannot retroactively qualify legacy unknown attempts.

The Git Slop Agent Plugin is a portable Agent Plugins package at
`plugins/git-slop/plugin.json`; Codex-specific presentation stays under its
`extensions.com.openai` metadata. Reusable project and product development
workflows come from the selected public plugins, while Git Slop-specific CLI
usage and adoption guidance stays in this repository's product plugin.

Execution State Sync is manual and runs only from the default branch. Choose
`operation=prepare` with exactly one whole-number `pr_number` or `issue_number`
to obtain a read-only preview. Review `previews/execution.json` in that run's
immutable handoff. A separate `operation=apply` dispatch names its
`plan_run_id` and exact `approve_plan_sha`; preparation never submits the
computed hash as its own approval. The apply package records the original
manual approval event and uses a native `plan-set-<digest>` recovery identity.
A retry restores those original approval bytes, plan and journal; conflicting
inputs retain diagnostics and stop before dispatch or settlement.

Preparation closes only its own positively observed preview-only no-op, using
the native receipt, artifact and checkpoint protocol. It does not settle old
unknown workflow attempts. Complete-history recovery still runs before any new
work, source allowlists remain empty, and dependency publication remains
disabled pending #157. Source validation does not authorize a live dispatch,
historical settlement, Project write, or host installation.
