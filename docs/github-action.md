# GitHub Action

The Git Slop Action publishes repository-health analysis without requiring
Python, Homebrew, Cargo, or a Rust toolchain in the consumer repository. It
downloads the requested prebuilt release, verifies the archive against the
release's `SHA256SUMS`, runs the detector once, and leaves both human and
machine-readable evidence available to later steps.

## Recommended Workflow

Git Slop uses commit history for churn, age, coupling, and maintenance-pressure
signals. Check out the complete history:

```yaml
name: Repository health

on:
  pull_request:
  push:
    branches: [main]
  schedule:
    - cron: "0 9 * * *"
  workflow_dispatch:

permissions:
  contents: read

jobs:
  git-slop:
    name: Git Slop
    runs-on: ubuntu-latest
    steps:
      - name: Checkout
        uses: actions/checkout@v7
        with:
          fetch-depth: 0

      - name: Analyze repository health
        id: git-slop
        uses: coreycoto/git-slop@v0.9.0
```

The default is advisory:

- `git-slop find` runs exactly once.
- `.slop/latest/health.md` is appended to the job summary.
- at most 10 workflow annotations are emitted.
- only `health.md` is uploaded as the `git-slop` artifact.
- the artifact is retained for 14 days.
- pull request comments are disabled.
- findings do not fail the job, but installation, shallow-history, detector, or
  renderer errors do.

The Action supports GitHub-hosted Linux x64/ARM64, macOS Intel/Apple Silicon,
and Windows x64 runners. The release must contain the matching
`git-slop-v<version>-<target>` archive and a `SHA256SUMS` file.

`working-directory` may point anywhere inside a worktree. Git Slop resolves
the worktree's top level and analyzes the complete tracked repository, matching
the CLI contract.

## Enforcement

Enable the stable detector gate on the same analysis step. The Action still
publishes the report and job summary before it evaluates the gate:

```yaml
      - name: Analyze and enforce repository health
        uses: coreycoto/git-slop@v0.9.0
        with:
          policy: enforce
```

`policy: enforce` runs `git-slop check --report .slop/latest/report.json` after
annotations, artifact upload, and optional comment publication. The default
thresholds come from `.slop/config.yaml`. A consumer can explicitly override
them:

```yaml
        with:
          policy: enforce
          fail-on-context-band: critical
          fail-on-slop-band: critical
```

Exit `0` passes, exit `1` means policy findings, and exit `2` means a usage or
input error. Overlay evidence enriches the report but does not silently change
the stable detector gate.

## Artifacts

`artifact-contents` always selects from a fixed allowlist; the Action never
uploads `.slop/latest/` or `.slop/runs/` as a directory.

| Value | Uploaded files |
| --- | --- |
| `summary` | `health.md` |
| `report` | `health.md`, `report.json` |
| `full` | `health.md`, `summary.md`, `report.json`, `report.yaml` |

For example:

```yaml
        with:
          artifact-contents: report
          retention-days: 14
```

Use `report.json` for automation. Markdown is the human-facing contract, while
the JSON `schema_version` is the machine compatibility boundary.

## Pull Request Comments

Job summaries and annotations require only `contents: read`. Pull request
comments are deliberately opt-in. When enabled, grant write permission and
understand that tokens on pull requests from forks may remain read-only:

```yaml
permissions:
  contents: read
  pull-requests: write

steps:
  - uses: actions/checkout@v7
    with:
      fetch-depth: 0
  - uses: coreycoto/git-slop@v0.9.0
    with:
      pr-comment: "true"
```

The Action creates or updates one marker-based comment instead of adding a new
comment on every run. Long reports are truncated in the comment; the complete
report remains in the job summary and artifact.

## Inputs

| Input | Default | Purpose |
| --- | --- | --- |
| `version` | `0.9.0` | Prebuilt release version to download and verify |
| `release-repository` | `coreycoto/git-slop` | Repository containing release assets |
| `working-directory` | `.` | Directory inside the Git worktree to analyze at its top level |
| `policy` | `advisory` | `advisory` or `enforce` |
| `fail-on-context-band` | empty | Optional check threshold override |
| `fail-on-slop-band` | empty | Optional check threshold override |
| `annotations` | `true` | Emit workflow annotations |
| `max-annotations` | `10` | Annotation cap from 0 through 50 |
| `upload-artifact` | `true` | Upload the bounded artifact |
| `artifact-name` | `git-slop` | Artifact name |
| `artifact-contents` | `summary` | `summary`, `report`, or `full` |
| `retention-days` | `14` | Artifact retention from 1 through 90 days |
| `pr-comment` | `false` | Update one pull request comment |
| `github-token` | `github.token` | Optional token override |

Useful outputs include `status`, `version`, `target`, `binary-path`,
`asset-sha256`, `analysis-exit-code`, `policy-exit-code`, `finding-count`,
`annotation-count`, report paths, artifact metadata, and `comment-url`.
