# CI and Distribution Troubleshooting

The Action needs `contents: read`; PR comments additionally need pull-request
write permission. Use `baseline-report` for regression-only annotations and
keep the absolute repository dashboard in the job summary. A baseline must come
from a compatible analyzer, configuration, and history contract.

## Fork pull requests

Keep `pull-requests: write` and `pr-comment` disabled for fork pull requests.
Job summaries, annotations, and artifacts work with `contents: read`. Prefer
`baseline-ref` with the event's base SHA so the Action scans the exact ancestor
in an isolated worktree without executing base-report content.

## Installation or provenance mismatch

The version string alone is insufficient. A published binary must return a
schema-2 `build-info` object whose version, source revision, target, crate
digest, and clean build-source identity match the release. Reinstall from one
verified distribution surface if a field is missing or mismatched.

When opening a bug, include version, redacted `build-info`, OS and architecture,
installation method, exact command, exit code, JSON error code and pointer, and
the doctor bundle. State whether `--ephemeral` reproduces the behavior in a
minimal repository.

For Action or archive verification failures, re-download the versioned asset,
`SHA256SUMS`, and `release-manifest.json`. Verify filename, size, SHA-256,
target, release-tag revision, canonical crate digest, and installed schema-2
`build-info` separately. Never bypass an archive-layout, digest, tag, or
crate-identity failure by disabling verification. The
[installation guide](../install.md) contains Unix and Windows recovery commands.
