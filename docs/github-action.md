# GitHub Action

The Git Slop Action publishes repository-health analysis without requiring a
separate package manager or toolchain in the consumer repository. It downloads
the requested prebuilt release, verifies the GitHub release and tag, schema-3
manifest, exact asset inventory, GitHub asset digests,
`SHA256SUMS`, crates.io package provenance, archive contents, and installed
`build-info`. It then runs the detector once and leaves both human and
machine-readable evidence available to later steps.

Crates provenance is verified independently of GitHub release assets. The
installer downloads the canonical static `.crate` without sending the GitHub
token, applies a 16 MiB download bound, checks its SHA-256 against the manifest,
and verifies the package's embedded clean VCS revision. Native archives and
their manifest entries are each limited to 128 MiB.

The examples below pin `v0.12.0`. Use them only after its verified GitHub
Release is public and the Marketplace listing resolves; a source tag or
documentation on `main` is not an availability proof.

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
        uses: coreycoto/git-slop@v0.12.0
```

The default is advisory:

- `git-slop find` runs exactly once.
- `.slop/latest/health.md` is appended to the job summary.
- at most 10 workflow annotations are emitted, preserving each finding's
  `notice`, `warning`, or `error` level.
- only `health.md` is uploaded as the `git-slop` artifact.
- the artifact is retained for 14 days.
- pull request comments are disabled.
- findings do not fail the job, but installation, shallow-history, detector, or
  renderer errors do.

The publication sequence is explicit:

1. Run `git-slop find` once, producing the persisted compact bundle.
2. Append the persisted `.slop/latest/health.md` to `GITHUB_STEP_SUMMARY`.
3. When annotations are enabled, run `git-slop health --report
   .slop/latest/report.json --format github --max-annotations <count>` and emit
   its standard output as bounded workflow annotations. This projection does
   not rewrite `health.md` or rerun `find`.
4. Publish the selected artifact and optional pull request comment, then, only
   for `policy: enforce`, apply either native `git-slop check` absolute
   thresholds or the already-produced native comparison regression count.

The dashboard and annotation findings are advisory projections. A successful
`health` render exits 0 even when findings are present; `check` is the step that
turns configured stable thresholds into an enforcing exit status.

### Finding Levels And Annotation Bounds

The health renderer owns finding severity, and the Action streams its bounded
workflow commands without reclassifying them:

| Rendered finding | GitHub workflow command |
| --- | --- |
| `notice` | `::notice` |
| `warning` | `::warning` |
| `error` | `::error` |

`max-annotations` caps the ordered finding stream as a whole; it is not a
per-level quota. An advisory run therefore does not turn an `error` into a
warning, and `policy: enforce` does not turn a notice into an error. Enforcement
is evaluated later by `git-slop check` against the same persisted
`report.json`.

The job-summary Markdown uses **context/load bands** for token and direct-folder
load, **maintenance-pressure** for stable `slop_score`/`slop_band` evidence,
and `notice`/`warning`/`error` for rendered review severity. Surfaced folder
rows name their exact crossed boundary, provide a folder-scoped
`git-slop explain --path <folder>/` command, and preview one deterministically
highest-ranked descendant. Number grouping and decimal precision are fixed by
the Markdown projection; machine-readable JSON values are unchanged.

The Action supports GitHub-hosted Linux x64/ARM64, macOS Apple Silicon, and
Windows x64/ARM64 runners. The release must contain the matching
`git-slop-v<version>-<target>` archive, `SHA256SUMS`,
`release-manifest.json`, and the crates-backed `git-slop.rb` Formula. The
Action installs the prebuilt native archive; it never invokes Homebrew or
compiles the crate on a consumer runner. Release automation builds that archive
from the exact `.crate` bytes recorded in the manifest.

`working-directory` may point anywhere inside a worktree. Git Slop resolves
the worktree's top level and analyzes the complete tracked repository, matching
the CLI contract.

## Provenance And Installation Failures

Installation fails before repository analysis if the requested stable version
is missing, is still a draft, has an unexpected asset inventory, resolves to a
different tag revision, contains a digest mismatch, or packages unsafe archive
members. It also fails when the installed binary's `build-info` does not report
the manifest revision with `source_dirty: false`. Tag resolution uses the exact
`refs/tags/vX.Y.Z` namespace and safely peels bounded annotated tags; a
same-named branch cannot satisfy the release identity. The release workflow
alone uses an explicit internal draft-verification mode before the human
Marketplace gate; consumer runs cannot opt into an unverified draft.

On success, record these outputs when downstream attestations need the release
identity:

- `source-revision`: full 40-character commit shared by the tag and binary
- `crate-sha256`: SHA-256 of the canonical static crates.io package
- `release-manifest-sha256`: SHA-256 of the schema-3 manifest
- `asset-sha256`: SHA-256 of the selected native archive

## Enforcement

Enable the stable detector gate on the same analysis step. The Action still
publishes the report and job summary before it evaluates the gate:

```yaml
      - name: Analyze and enforce repository health
        uses: coreycoto/git-slop@v0.12.0
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

For a regression ratchet, supply a compatible baseline and select native
regression enforcement:

```yaml
        with:
          policy: enforce
          enforcement: regression
          baseline-report: .ci/git-slop-baseline.json
          max-baseline-age-days: 30
```

The Action invokes `git-slop compare`; it has no second JavaScript comparator.
Scope, tokenizer, analyzer, config, repository, and history mismatches fail
closed. `baseline-force: "true"` records and permits exact intentional mismatches.

## Artifacts

`artifact-contents` always selects from a fixed allowlist; the Action never
uploads `.slop/latest/` or `.slop/runs/` as a directory.

| Value | Uploaded files |
| --- | --- |
| `summary` | `health.md` |
| `report` | `health.md`, `report.json`, plus baseline `comparison.json` |
| `full` | Report set plus `summary.md` and enabled `report.yaml` |

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
  - uses: coreycoto/git-slop@v0.12.0
    with:
      pr-comment: "true"
```

The Action creates or updates one marker-based comment instead of adding a new
comment on every run. Long reports are truncated in the comment; the complete
report remains in the job summary and artifact.

## Inputs

| Input | Default | Purpose |
| --- | --- | --- |
| `version` | `0.12.0` | Prebuilt release version to download and verify after that release is public |
| `mode` | empty | Simple preset: `advisory`, `absolute`, or `regression`; when set, it overrides `policy` and `enforcement` only |
| `release-repository` | `coreycoto/git-slop` | Repository containing release assets |
| `target` | empty | Compatible release target override |
| `working-directory` | `.` | Directory inside the Git worktree to analyze at its top level |
| `scope` | empty | Optional repository-relative analysis scope |
| `report-profile` | `standard` | `compact`, `standard`, or `full-evidence` report semantics |
| `compression` | `none` | Optional `gzip` or `zstd` report companion |
| `token-cache` | `false` | Opt in to a content-addressed token cache persisted with `actions/cache` |
| `policy` | `advisory` | `advisory` or `enforce` |
| `enforcement` | `absolute` | Absolute thresholds or a native `regression` ratchet |
| `baseline-report` | empty | Compatible base report for annotations or enforcement |
| `baseline-ref` | empty | Git revision or SHA scanned in an isolated baseline worktree |
| `baseline-force` | `false` | Record and allow exact compatibility mismatches |
| `max-baseline-age-days` | `30` | Reject stale baseline evidence |
| `require-baseline-ancestor` | `true` | Require ancestor evidence for regression enforcement unless forced |
| `allow-shallow` | `false` | Explicitly accept incomplete shallow history |
| `fail-on-context-band` | empty | Optional check threshold override |
| `fail-on-slop-band` | empty | Optional check threshold override |
| `annotations` | `true` | Emit workflow annotations |
| `max-annotations` | `10` | Ordered annotation cap from 0 through 50; finding levels are preserved |
| `upload-artifact` | `true` | Upload the bounded artifact |
| `artifact-name` | `git-slop` | Artifact name |
| `artifact-contents` | `summary` | `summary`, `report`, or `full` |
| `retention-days` | `14` | Artifact retention from 1 through 90 days |
| `pr-comment` | `false` | Update one pull request comment |
| `github-token` | `github.token` | Optional token override |

## Outputs

This table is contract-checked against `action.yml`; adding or removing Action
metadata without updating the documentation fails repository validation.

| Output | Purpose |
| --- | --- |
| `status` | Final Action status |
| `version` | Installed Git Slop version |
| `target` | Installed Rust target triple |
| `binary-path` | Absolute verified executable path |
| `asset-sha256` | Verified native archive digest |
| `source-revision` | Full verified source revision |
| `crate-sha256` | Verified crates.io package digest |
| `release-manifest-sha256` | Verified release-manifest digest |
| `cache-hit` | Whether the verified executable was reused from `RUNNER_TOOL_CACHE` |
| `analysis-exit-code` | Exit code from the single analysis invocation |
| `mode` | Effective simple preset, or `advanced` when using policy and enforcement directly |
| `analysis-error-path` | Preserved post-analysis diagnostic path |
| `policy-exit-code` | Exit code from the selected policy gate |
| `finding-count` | Deprecated alias for `selected-policy-finding-count`; removed in v0.13.0, no earlier than 2026-11-01 |
| `health-finding-count` | Uncapped actionable head-health findings |
| `policy-finding-count` | Deprecated alias for `selected-policy-finding-count`; removed in v0.13.0, no earlier than 2026-11-01 |
| `absolute-finding-count` | Findings from the absolute head gate |
| `selected-policy-finding-count` | Findings selected by enforcement mode |
| `regression-count` | Native comparator regressions |
| `baseline-compatible` | Deprecated compatibility boolean; removed in v0.13.0, no earlier than 2026-11-01; use `baseline-status` |
| `baseline-status` | Structured baseline evaluation state |
| `comparison-path` | Native comparison JSON path |
| `comparison-error-path` | Preserved baseline-comparison diagnostic path |
| `annotation-count` | Number of emitted annotations |
| `health-path` | Absolute `health.md` path |
| `report-path` | Absolute `report.json` path |
| `report-yaml-path` | Absolute optional `report.yaml` path |
| `compressed-report-path` | Absolute optional `.gz` or `.zst` report path |
| `summary-path` | Absolute `summary.md` path |
| `artifact-id` | Uploaded artifact ID |
| `artifact-url` | Uploaded artifact URL |
| `artifact-digest` | Uploaded artifact digest |
| `comment-url` | Created or updated pull-request comment URL |

`cache-hit` reports whether the fully verified binary was reused from
`RUNNER_TOOL_CACHE`; release metadata, manifest, checksums, Formula, tag, and
cached binary identity are still revalidated on every run. The provenance
outputs let a consuming workflow record the same source revision and crate
digest used by crates.io, GitHub Releases, the Marketplace Action, and
Homebrew.

## GitHub Marketplace

The Action will be published from this repository's verified stable GitHub
Release under the **Code quality** and **Continuous integration** categories.
That first listing requires a maintainer to select GitHub's Marketplace checkbox
in the draft-release UI, confirm the categories and agreement, and complete
2FA. Marketplace and direct `uses: coreycoto/git-slop@v0.12.0` installation then
resolve the same root `action.yml` and release provenance. For
higher-assurance consumers, pin the Action itself to the full release commit
SHA; the Action's own nested dependencies are already pinned to full commit
SHAs.
