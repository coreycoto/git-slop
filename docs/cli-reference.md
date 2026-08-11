# Git Slop CLI Reference

Generated from the live Clap command tree.

## `git-slop`

Find the files that cost too much context.

| Argument | Description |
| --- | --- |
| `--repo` | Repository or path inside a repository to analyze |
| `--error-format` | Render runtime errors as human text or stable JSON |

## `git-slop init`

Scaffold .slop/ config, ignore rules, and state directories

| Argument | Description |
| --- | --- |
| `--force` | Overwrite generated config files |

## `git-slop find`

Scan the repository and generate hotspot reports

| Argument | Description |
| --- | --- |
| `--allow-shallow` | Acknowledge incomplete history and continue in a shallow clone |
| `--scope` | Analyze only this repo-relative path while retaining repository-wide Git evidence |
| `--allow-empty-scope` | Permit a scope that selects no tracked paths and emit an empty analysis |
| `--quiet` | Suppress human progress and report-path messages |
| `--no-progress` | Suppress phase progress while preserving the final result |
| `--state-dir` | Mutable cache/state directory. Relative paths resolve from the repository root |
| `--output-dir` | Report output directory. Relative paths resolve from the repository root |
| `--no-cache` | Disable token-cache reads and writes for an ephemeral scan |
| `--allow-degraded` | Deterministically analyze the largest path prefix that fits the memory budget |
| `--as-of` | Fixed RFC 3339 analysis clock for reproducible recency and history windows |
| `--report-profile` | Report evidence profile |
| `--compression` | Also write a compressed report beside report.json |
| `--estimate-only` | Estimate scope, memory, cache, report size, time, and inodes without scanning |

## `git-slop show`

Show metrics and reasons for one file or folder

| Argument | Description |
| --- | --- |
| `target_path` | Repo-relative file or folder path |
| `--report` | Report path. Defaults to .slop/latest/report.json |
| `--format` | Output format |

## `git-slop explain`

Explain why selected hotspots or structural findings are expensive

| Argument | Description |
| --- | --- |
| `--report` | Report path. Defaults to .slop/latest/report.json |
| `--path` | Repo-relative file or folder path |
| `--cluster` | Cluster identifier |
| `--relationship` | Relationship identifier |
| `--top` | Explain the top N hotspots from the action queue |
| `--format` | Output format |
| `--prompt-pack` | Write a deterministic local-model prompt pack to this directory |
| `--force` | Atomically replace an existing prompt-pack directory |
| `--include-repository-context` | Include bounded local source/test excerpts, guidance, and verification hints |
| `--excerpt-bytes` | Maximum bytes read from each included repository file |
| `--include-local-paths` | Include local filesystem paths in prompt-pack provenance and commands |

## `git-slop plan`

Propose bounded maintenance slices from the current detector report

| Argument | Description |
| --- | --- |
| `--report` | Report path. Defaults to .slop/latest/report.json |
| `--path` | Repo-relative file or folder path |
| `--cluster` | Cluster identifier |
| `--relationship` | Relationship identifier |
| `--max-slices` | Maximum number of bounded maintenance slices to propose |
| `--format` | Output format |
| `--prompt-pack` | Write a deterministic local-model prompt pack to this directory |
| `--force` | Atomically replace an existing prompt-pack directory |
| `--include-repository-context` | Include bounded local source/test excerpts, guidance, and verification hints |
| `--excerpt-bytes` | Maximum bytes read from each included repository file |
| `--include-local-paths` | Include local filesystem paths in prompt-pack provenance and commands |

## `git-slop check`

Evaluate an existing report against CI thresholds

| Argument | Description |
| --- | --- |
| `--report` | Report path. Defaults to .slop/latest/report.json |
| `--fail-on-context-band` | Override the config default fail threshold for context_band |
| `--fail-on-slop-band` | Override the config default fail threshold for slop_band |
| `--format` | Output format, including escaped GitHub workflow commands |
| `--details` | Include complete finding records in JSON output |
| `--include-folders` | Include folder records in addition to the versioned file-only gate |
| `--offset` | Zero-based finding offset used with --details |
| `--limit` | Maximum finding records returned with --details |
| `--allow-incomplete-evidence` | Permit policy evaluation when selected inventory records are incomplete |
| `--evaluate-only` | Evaluate and report the canonical policy result without returning exit 1 for findings |

## `git-slop compare`

Compare two existing schema-5 reports without rerunning the detector

| Argument | Description |
| --- | --- |
| `--base` | Base report.json path |
| `--base-ref` | Safely resolve and scan this Git revision in an isolated worktree |
| `--baseline` | Use a named baseline from Git-private runtime storage |
| `--head` | Head report.json path |
| `--scope` | Apply the head repository's scope to an isolated --base-ref scan |
| `--allow-shallow` | Permit incomplete history in an isolated --base-ref scan |
| `--allow-incomplete-evidence` | Permit comparison when selected inventory records are incomplete |
| `--top` | Maximum number of changed files and queue movements to show |
| `--format` | Output format |
| `--detail` | Detail level for machine output |
| `--offset` | Zero-based record offset for --detail full |
| `--limit` | Maximum records per collection for --detail full |
| `--force` | Compare reports with incompatible identity or analyzer metadata |
| `--include-local-paths` | Include local filesystem report paths in output descriptors |
| `--include-unchanged` | Include unchanged file and folder records in bounded compare collections |
| `--policy-from` | Select which report supplies regression thresholds and evidence-drift policy |
| `--fail-on-regression` | Exit 1 when an existing file worsens or a newly added file is a finding |

## `git-slop baseline`

Manage named comparison baselines in Git-private runtime storage

## `git-slop baseline ensure`

Idempotently save a named baseline, failing closed when stored content differs

| Argument | Description |
| --- | --- |
| `--name` | Stable baseline name |
| `--report` | Report path. Defaults to .slop/latest/report.json |
| `--replace` | Explicitly replace a differing stored baseline |
| `--allow-dirty` | Permit a report produced from a dirty worktree |
| `--allow-incomplete-evidence` | Permit incomplete inventory or history evidence |
| `--format` | Output format |

## `git-slop baseline create`

Create a named baseline from a validated report

| Argument | Description |
| --- | --- |
| `--name` | Stable baseline name |
| `--report` | Report path. Defaults to .slop/latest/report.json |
| `--force` | Replace an existing named baseline |
| `--allow-dirty` | Permit a report produced from a dirty worktree |
| `--allow-incomplete-evidence` | Permit incomplete inventory or history evidence |
| `--format` | Output format |

## `git-slop baseline update`

Replace an existing named baseline from a validated report

| Argument | Description |
| --- | --- |
| `--name` | Stable baseline name |
| `--report` | Report path. Defaults to .slop/latest/report.json |
| `--allow-dirty` | Permit a report produced from a dirty worktree |
| `--allow-incomplete-evidence` | Permit incomplete inventory or history evidence |
| `--format` | Output format |

## `git-slop baseline list`

List named baselines with identity and readiness metadata

| Argument | Description |
| --- | --- |
| `--format` | Output format |

## `git-slop baseline inspect`

Inspect baseline identity and evidence status

| Argument | Description |
| --- | --- |
| `--name` | Stable baseline name |
| `--format` | Output format |

## `git-slop baseline validate`

Validate a named baseline against the current report contract

| Argument | Description |
| --- | --- |
| `--name` | Stable baseline name |
| `--format` | Output format |

## `git-slop baseline remove`

Remove a named baseline

| Argument | Description |
| --- | --- |
| `--name` | Stable baseline name |
| `--format` | Output format |

## `git-slop report`

Validate or inspect the versioned report contract

## `git-slop report validate`

Validate one report against the complete schema-5 contract

| Argument | Description |
| --- | --- |
| `path` | Report JSON to validate |
| `--allow-legacy` | Accept schema 4 as migration input and validate its normalized schema-5 form |

## `git-slop report migrate`

Migrate a schema-4 report to normalized schema 5

| Argument | Description |
| --- | --- |
| `path` | Legacy report to migrate |
| `--output` | Destination for the normalized schema-5 report |

## `git-slop report schema`

Print the published JSON Schema for report schema 5

## `git-slop sarif`

Export action-queue findings from an existing schema-5 report as SARIF

| Argument | Description |
| --- | --- |
| `--report` | Report path. Defaults to .slop/latest/report.json |
| `--top` | Maximum number of action-queue findings to export |
| `--output` | Optional SARIF output path. Defaults to stdout |
| `--include-local-paths` | Include the local source report path in SARIF invocation properties |

## `git-slop health`

Render repository health for CI summaries, annotations, or automation

| Argument | Description |
| --- | --- |
| `--report` | Report path. Defaults to .slop/latest/report.json |
| `--format` | Output suited for a job summary, workflow annotations, or automation |
| `--max-annotations` | Maximum number of GitHub workflow annotations to emit |

## `git-slop config`

Inspect or migrate effective configuration

## `git-slop config show`

Show configuration; --effective includes defaults

| Argument | Description |
| --- | --- |
| `--effective` | Include defaults after applying repository overrides |

## `git-slop config validate`

Validate the local configuration

## `git-slop config diff-defaults`

Show only values that differ from defaults

## `git-slop config migrate`

Rewrite legacy schema configuration as a minimal schema-2 override

## `git-slop config schema`

Print the supported configuration schema as JSON

## `git-slop doctor`

Diagnose repository readiness and optionally write a redacted bundle

| Argument | Description |
| --- | --- |
| `--bundle` | Write a privacy-safe diagnostic JSON bundle |
| `--format` | Output format |
| `--scope` | Estimate only this repo-relative scope |

## `git-slop list`

List findings, relationships, clusters, or profiles

## `git-slop list findings`

| Argument | Description |
| --- | --- |
| `--report` | Report path. Defaults to .slop/latest/report.json |
| `--path` | Match a finding path, relationship endpoint, or cluster member |
| `--profile` | Match an analysis profile |
| `--language` | Match a resolved file language |
| `--classification` | Match a resolved file classification |
| `--severity` | Match a finding severity |
| `--top` | Maximum number of matched records to return |
| `--format` | Output format |
| `--wide` | Use a wider terminal layout before truncating fields |
| `--no-truncate` | Never truncate terminal fields |

## `git-slop list relationships`

| Argument | Description |
| --- | --- |
| `--report` | Report path. Defaults to .slop/latest/report.json |
| `--path` | Match a finding path, relationship endpoint, or cluster member |
| `--profile` | Match an analysis profile |
| `--language` | Match a resolved file language |
| `--classification` | Match a resolved file classification |
| `--severity` | Match a finding severity |
| `--top` | Maximum number of matched records to return |
| `--format` | Output format |
| `--wide` | Use a wider terminal layout before truncating fields |
| `--no-truncate` | Never truncate terminal fields |

## `git-slop list clusters`

| Argument | Description |
| --- | --- |
| `--report` | Report path. Defaults to .slop/latest/report.json |
| `--path` | Match a finding path, relationship endpoint, or cluster member |
| `--profile` | Match an analysis profile |
| `--language` | Match a resolved file language |
| `--classification` | Match a resolved file classification |
| `--severity` | Match a finding severity |
| `--top` | Maximum number of matched records to return |
| `--format` | Output format |
| `--wide` | Use a wider terminal layout before truncating fields |
| `--no-truncate` | Never truncate terminal fields |

## `git-slop list profiles`

| Argument | Description |
| --- | --- |
| `--report` | Report path. Defaults to .slop/latest/report.json |
| `--path` | Match a finding path, relationship endpoint, or cluster member |
| `--profile` | Match an analysis profile |
| `--language` | Match a resolved file language |
| `--classification` | Match a resolved file classification |
| `--severity` | Match a finding severity |
| `--top` | Maximum number of matched records to return |
| `--format` | Output format |
| `--wide` | Use a wider terminal layout before truncating fields |
| `--no-truncate` | Never truncate terminal fields |

## `git-slop prune`

Remove old immutable run snapshots according to retention policy

| Argument | Description |
| --- | --- |
| `--keep` | Number of newest run snapshots to retain; defaults to output.retention_runs |
| `--max-bytes` | Maximum total bytes retained; defaults to output.retention_bytes |
| `--dry-run` | Print removals without changing files |
| `--format` | Select text, JSON, or YAML output |

## `git-slop cache`

Inspect or prune the packed token cache

| Argument | Description |
| --- | --- |
| `--state-dir` | Mutable state directory. Defaults to Git-private runtime storage |

## `git-slop cache status`

| Argument | Description |
| --- | --- |
| `--format` | Output format |

## `git-slop cache prune`

| Argument | Description |
| --- | --- |
| `--max-entries` | Maximum entries to retain |
| `--max-bytes` | Maximum logical payload bytes to retain |
| `--dry-run` | Preview cache removals without changing the database |
| `--compact` | Reclaim free database pages after pruning |
| `--format` | Output format |

## `git-slop completions`

Generate shell completion source

| Argument | Description |
| --- | --- |
| `shell` | Shell whose completion source should be generated |

## `git-slop man`

Generate the roff manual from the live Clap command tree

| Argument | Description |
| --- | --- |
| `--output` | Destination file. Defaults to stdout |

## `git-slop reference`

Generate Markdown command reference from the live Clap command tree

| Argument | Description |
| --- | --- |
| `--output` | Destination file. Defaults to stdout |

## `git-slop html`

Write a self-contained, local, searchable HTML report

| Argument | Description |
| --- | --- |
| `--report` | Report path. Defaults to .slop/latest/report.json |
| `--output` | Destination. Defaults to .slop/latest/report.html |
| `--include-local-paths` | Embed the local source report path in the otherwise portable HTML file |

## `git-slop version`

Print version information

## `git-slop build-info`

Print package and source-build provenance

| Argument | Description |
| --- | --- |
| `--format` | Machine-readable build provenance format |

## `git-slop schema`

Print a published JSON Schema for a machine contract

| Argument | Description |
| --- | --- |
| `contract` | Machine contract whose immutable schema should be printed |
| `--output` | Destination file. Defaults to stdout |
