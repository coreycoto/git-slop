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
receipt and staged binary before any native workflow command runs. The current
lock names the published v0.1.0 assets. That executable lacks the new `runs`
commands, so the prepared recovery workflows cannot be promoted until the
additional release is qualified and the lock uses its actual source and hashes.

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
