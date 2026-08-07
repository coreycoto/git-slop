# Agent Surface

This directory carries two separate plugin contracts:

- the tracked marketplace-source contract for the installed shared
  project-management plugin
- the local Codex marketplace that distributes the portable `git-slop` Agent
  Plugin

Use these surfaces:

- `AGENTS.md`: always-on repo policy
- `.agents/plugins/marketplace-source.json`: pinned marketplace source manifest
- `.agents/plugins/marketplace.json`: local Codex marketplace for the portable `git-slop` Agent Plugin
- `.codex/README.md`: Codex runtime map

`git-slop` consumes the `project-management-workflows` plugin from
`coreycoto/agent-plugins` through this pinned manifest. The consumer pins the
publisher source revision, release identity, Linux target, archive member, and
SHA-256 digest. `scripts/with-agent-plugins.sh --prepare` downloads the private
PEX SCIE into an ephemeral per-job directory, and a separate `--verify` rejects
release metadata, embedded revision, digest, or archive-safety mismatches before
execution. The read token exists only during acquisition. The canonical
commands are `marketplace`, `github project-snapshot`, and
`github execution-state`; interpreter mode is internal to runtime identity
verification and the legacy compatibility entry point.

The verified runtime embeds the marketplace payload, so later marketplace
installation is offline and no command clones the publisher repository.
Execution-state governance commands still use the resolved project token for
their intended API calls, but that PAT is scoped only to the two direct
operation steps. It is absent from preparation, verification, and the publisher
identity and interpreter smoke checks. No Actions cache, consumer language
setup, or dependency environment is part of this contract. Bootstrap
implementation, reusable behavior tests, and clean-room consumer smoke coverage
stay in `agent-plugins`, not this consumer repository.

The public Agent Plugins specification and the private repository named
`coreycoto/agent-plugins` are separate contracts. The former defines the
portable `plugins/git-slop/plugin.json` package layout; the latter publishes
the shared project-management runtime consumed by this repository.

Repo-owned validation belongs to the private standalone Rust `xtask/`
workspace. Do not add a consumer project dependency sync or duplicated
publisher implementation here.

`git-slop` also publishes its repo-local Agent Plugin from `plugins/git-slop`.
Its root `plugin.json` targets Agent Plugins 1.0.0, while this local marketplace
is the Codex distribution layer. Codex CLI 0.146.0 or newer is required because
that release first loads Agent Plugins manifests. A metadata-only
`.codex-plugin/plugin.json` remains as a temporary Codex 0.146.x compatibility
overlay because 0.146.0 and 0.146.1 do not expose the complete root metadata
through `plugin/read`; the root manifest remains authoritative and `xtask`
requires an exact mirror with no component declarations. The plugin owns
portable product-specific guidance for installing, running, reviewing and
optionally planning from, and adopting the `git-slop` CLI. Its
`extensions.com.openai` manifest namespace carries only Codex UI metadata;
per-skill `agents/openai.yaml` files likewise add only OpenAI presentation and
the shared Git Slop icon. VS Code, Cursor, GitHub Copilot, and Kiro consume the
same root manifest and portable `SKILL.md` files without parallel vendor skill
copies; none currently defines an equivalent packaged per-skill icon overlay.
It should reference
`project-management-workflows` only when reviewed `git-slop` output is being
converted into backlog or governance work.
