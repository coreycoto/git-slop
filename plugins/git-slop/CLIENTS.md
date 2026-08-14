# Client Installation Recipes

Pin `<release>` to an immutable Git Slop tag and review the plugin before
installation. The native `git-slop` executable is still required; the plugin
supplies agent guidance, not a second runtime.

## ChatGPT and Codex

```bash
codex plugin marketplace add coreycoto/git-slop --ref <release>
codex plugin add git-slop@git-slop-marketplace
codex plugin list --json
```

Verify that the four `git-slop` skills are listed, then ask Codex to run
`git slop doctor`. The `.codex-plugin/plugin.json` file is metadata-only; the
portable root manifest and `skills/` directory remain authoritative.

Refresh the pinned marketplace, then reinstall the plugin from that snapshot:

```bash
codex plugin marketplace upgrade git-slop-marketplace
codex plugin remove git-slop@git-slop-marketplace
codex plugin add git-slop@git-slop-marketplace
codex plugin list --json
```

Remove both the installed plugin and, when no longer needed, its marketplace:

```bash
codex plugin remove git-slop@git-slop-marketplace
codex plugin marketplace remove git-slop-marketplace
```

These commands are contract-tested against the repository metadata and were
checked against the shipped Codex CLI 0.147.0 command help.

## GitHub Copilot CLI and VS Code

Copilot CLI accepts a GitHub repository subdirectory directly:

```bash
copilot plugin install coreycoto/git-slop:plugins/git-slop
copilot plugin list
copilot plugin update git-slop
copilot plugin uninstall git-slop
```

VS Code automatically discovers Copilot CLI plugins under
`~/.copilot/installed-plugins/`. In VS Code, enable agent plugins, open the
Extensions view, search `@agentPlugins`, and confirm Git Slop appears in the
installed list. Invoke `/git-slop:run-report` or ask for a Git Slop report.
Use `copilot plugin update git-slop` before verifying an updated pin; removal
from Copilot CLI also removes the copy discovered by VS Code.

See the official [Copilot CLI plugin reference](https://docs.github.com/en/copilot/reference/copilot-cli-reference/cli-plugin-reference)
and [VS Code agent-plugin guide](https://code.visualstudio.com/docs/agent-customization/agent-plugins).

## Cursor

Cursor consumes the portable Agent Skills inside the plugin. From a pinned
checkout, install them at workspace scope:

```bash
mkdir -p .cursor/skills
cp -R plugins/git-slop/skills/adopt-repo .cursor/skills/
cp -R plugins/git-slop/skills/install-update .cursor/skills/
cp -R plugins/git-slop/skills/review-results .cursor/skills/
cp -R plugins/git-slop/skills/run-report .cursor/skills/
```

Restart or reload Cursor, confirm the four skills appear, and invoke
`/run-report`. User-wide installation uses the same directories under
`~/.cursor/skills/`. Cursor's published Agent Skills support is described in
its [2.4 release notes](https://cursor.com/changelog/2-4).

To update, remove only these four exact directories and repeat the copy from a
new pinned checkout. To uninstall, remove those same directories and reload
Cursor:

```bash
rm -rf .cursor/skills/adopt-repo .cursor/skills/install-update \
  .cursor/skills/review-results .cursor/skills/run-report
```

## Kiro

In Kiro's **Agent Steering & Skills** panel, choose **Import a skill** and
import each pinned skill-folder URL, for example:

```text
https://github.com/coreycoto/git-slop/tree/<release>/plugins/git-slop/skills/run-report
```

Repeat for `adopt-repo`, `install-update`, and `review-results`. Workspace
imports land under `.kiro/skills/`; global imports use `~/.kiro/skills/`.
Confirm `/run-report` is available. See Kiro's official
[Agent Skills guide](https://kiro.dev/docs/skills/).

To update, remove each imported Git Slop skill in **Agent Steering & Skills**,
then import the four URLs at the new immutable release. To uninstall, remove
those four workspace or global skills in the same panel and confirm
`/run-report` is no longer discoverable.

## Acceptance check for every client

1. Confirm all four skill names are discoverable.
2. Ask for installation verification; the skill should run `git-slop version`
   and `git-slop build-info --format json` rather than guessing availability.
3. Ask for a fresh report; the skill should run `find` once and consume the
   resulting report for health, explain, plan, compare, or check.
4. Confirm no client-specific copy changes detector scoring, mutates source, or
   treats a finding as proof that a refactor is correct.

Repository maintainers run `cargo xtask validate-codex` to verify these recipe
commands, all four skill directories, the authoritative manifest, and the
metadata-only Codex compatibility mirror together.
