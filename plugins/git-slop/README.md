# Git Slop Agent Plugin

This portable Agent Plugin is the guidance surface for the `git-slop` product
CLI. Its root `plugin.json` follows the
[Agent Plugins 1.0.0 specification](https://agent-plugins.org/specification),
its skills are portable, and Codex-specific UI metadata is isolated under
`extensions.com.openai`.

## Portable Contract

The package follows the standard discovery layout:

```text
plugins/git-slop/
├── plugin.json
├── .codex-plugin/plugin.json
├── skills/*/SKILL.md
├── skills/*/agents/openai.yaml
├── skills/*/assets/git-slop.svg
└── assets/git-slop.svg
```

Agent Plugins clients discover the immediate child skill directories from the
root without a manifest-level `skills` field. This plugin has no MCP server and
therefore does not publish `mcp.json`. The root `plugin.json` is authoritative.
Its package version stays exactly aligned with the public `git-slop` crate and
release version; `cargo xtask validate-codex` rejects version drift.
Portable skill matching and workflow behavior live in `SKILL.md`. The optional
`agents/openai.yaml` files add OpenAI UI labels, starter prompts, the shared Git
Slop icon, and an explicit `allow_implicit_invocation: true` policy so each
well-bounded skill works without an explicit `$skill` mention. Other Agent
Skills clients can ignore them.

## Compatible Clients

The package targets the five clients in the
[Agent Plugins compatibility matrix](https://agent-plugins.org/compatible-clients).
All five load the same portable root manifest and immediate
`skills/*/SKILL.md` directories; no client-specific copy of a skill is needed.

Exact installation and acceptance checks differ by client. Use the tested
[client recipes](CLIENTS.md); do not infer one client's marketplace commands
from another client's discovery support.

| Client | Portable surface | Client-specific presentation |
| --- | --- | --- |
| ChatGPT & Codex | `plugin.json` and `skills/*/SKILL.md` | `extensions.com.openai`, each skill's optional `agents/openai.yaml`, and the temporary metadata-only Codex overlay |
| VS Code | `plugin.json` and `skills/*/SKILL.md` | None; VS Code currently ignores Agent Plugins client-extension data and directories |
| Cursor | `plugin.json` and `skills/*/SKILL.md` | None; conformant Agent Plugins load without a Cursor-specific manifest |
| GitHub Copilot | `plugin.json` and `skills/*/SKILL.md` | None; the canonical `$schema` opts the package into Agent Plugins 1.0 semantics |
| Kiro | `plugin.json` and `skills/*/SKILL.md` | None; Kiro consumes the portable Agent Skills component |

Agent Plugins 1.0 and the Agent Skills specification do not define a portable
plugin- or skill-icon field. The package therefore carries the same 64x64 Git
Slop SVG at the plugin root and in every skill, while only the documented OpenAI
interface metadata points to it today. OpenAI presentation surfaces use
`#6f42c1`, the purple behind Git Slop's GitHub Actions Marketplace icon, and the
SVG inherits its foreground color so each client can keep the glyph legible. Do
not add guessed `agents/vscode.yaml`,
`agents/cursor.yaml`, `agents/github.yaml`, or `agents/kiro.yaml` files. Add a
new client overlay only when that client publishes a namespaced extension or
per-skill presentation schema; the portable behavior must remain in
`SKILL.md`.

The metadata-only `.codex-plugin/plugin.json` is a temporary Codex 0.146.x
compatibility overlay. Released Codex 0.146.0 and 0.146.1 load the portable
skills but do not expose all portable or `extensions.com.openai` metadata from
the root manifest through `plugin/read`. The overlay exactly mirrors that
metadata and declares no skills, MCP servers, apps, or hooks. Remove it only
after a shipped Codex release resolves the root manifest completely and the
clean-room app-server proof passes without it.

Codex consumers install the portable plugin through the repo's stable local
marketplace name:

```bash
codex plugin marketplace add coreycoto/git-slop --ref <release>
codex plugin add git-slop@git-slop-marketplace
```

These commands require Codex CLI 0.146.0 or newer, the first Codex release that
loads Agent Plugins manifests. Pin `<release>` to an immutable published tag.
The marketplace is Codex's distribution layer; the installed package itself
remains an Agent Plugins 1.0.0 package.

The Agent Plugins specification is not the same thing as the separate private
repository named `coreycoto/agent-plugins`; that repository continues to own
shared project-management workflow guidance and its prebuilt runtime.

It covers:

- installing and updating the native Rust CLI
- running report, health, explain, plan, and check commands
- reviewing `.slop/latest/` artifacts and optionally planning one bounded slice
- preserving `.slop/` generated-state boundaries
- adopting `git-slop` locally and through its GitHub Action

It intentionally does not own generic backlog, release, project, or governance
workflows. When a reviewed `git-slop plan` should become backlog work, use the
separate `project-management-workflows` plugin from `coreycoto/agent-plugins`.

The public `git-slop` runtime is a native executable. `find` always writes
schema-5 JSON plus detailed `summary.md` and CI-oriented `health.md`; YAML is
written only when `output.yaml: true`. Stable costs
drive the existing `check` gate; overlays and health rollups remain additive
evidence.

For the stable distribution contract, the published crates.io package is the
canonical source identity. The Homebrew Formula installs that exact crate, with
bottles serving only as faster transport. The public Action and external Scoop
bucket install checksummed GitHub Release archives verifiably bound to the same
crate and source revision; Scoop publishes only after trusted x64 and ARM64
qualification. Describe availability only for distribution surfaces that have
been published and verified. Verify a published binary with
`git-slop build-info --format json`.

Product guidance should treat `.slop/latest/`, `.slop/runs/`, `.slop/cache/`,
prompt packs, SARIF exports, plan JSON, and compare JSON as generated artifacts
unless a repository intentionally curates them as fixtures outside `.slop/`.
The GitHub Action uploads only an allowlisted subset, with `health.md` as its
default artifact.
The portable Agent Plugin layout can be installed by clients that support
Agent Plugins or Agent Skills, including Codex, VS Code/Copilot, Cursor, and
Kiro. Point the client at `plugins/git-slop`, keep `plugin.json` authoritative,
and follow that client's local plugin installation flow. The
`.codex-plugin/plugin.json` file is only a metadata compatibility mirror.
