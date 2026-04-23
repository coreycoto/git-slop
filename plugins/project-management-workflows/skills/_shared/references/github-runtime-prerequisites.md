# GitHub Runtime Prerequisites

GitHub-touching local interactive skills in this plugin require both:

- the official GitHub Codex plugin (`github@openai-curated`)
- the GitHub connector mapping bundled by this plugin via `plugins/project-management-workflows/.app.json`

The connector mapping does not replace the official GitHub plugin. The official
plugin remains a separate prerequisite because it provides the GitHub plugin
surface expected by these local interactive workflows.

Run `python3 ../../scripts/preflight_github_surface.py` from a plugin skill
directory when you need to confirm the combined GitHub runtime surface before
continuing.

CI and GitHub Actions do not rely on marketplace-installed connectors. Those
automation paths continue to use checked-out repo files, `gh`, `agent-tools`,
local scripts, prompt files, and GitHub tokens instead.

When either prerequisite is missing or disabled, GitHub-touching local
interactive skills should stop immediately and fail with an actionable error
instead of degrading into best-effort GitHub behavior.
