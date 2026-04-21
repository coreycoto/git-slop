# GitHub Mutation Contract

Use live GitHub mutation only after deterministic local validation.

Rules:

- validate checked-in config before touching GitHub
- prefer preview and diff artifacts before apply
- keep issue bodies concise and push bulky evidence into repo-local artifacts
- do not auto-change parent/sub-issue links, issue milestones, or project queue order without explicit review
- safe auto-fixes are limited to repo-managed labels and quarter milestone drift
