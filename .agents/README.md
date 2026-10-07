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
extension installation. The current
lock names the qualified published v0.5.0 assets at source
`a250444eb9187cd90d24aa5af82ac963b3753fa4`, including the shared `runs`
commands. All four native release gates, independent tagged rebuilds, checksums
and attestations passed. Consumer workflow promotion still requires its own
hosted qualification and authorization.

Merge On Green uses [#173](https://github.com/coreycoto/git-slop/issues/173)
as its independent baseline and promotion review channel. The policy selects
only that workflow and retains its existing `merge` plan and disabled
publication. Capture the complete idle history with the pinned release, present
the exact baseline for human review, then prepare and separately review the
fresh-native promotion. Configuring these channels does not admit either
artifact or activate a recovery chain. Preserve failed diagnostics and every
historical unknown; never replay an old attempt to clear a hold.

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
