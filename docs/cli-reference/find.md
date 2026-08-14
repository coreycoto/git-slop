# Git Slop CLI Reference: `find`

Generated from the live Clap command tree.

## `git-slop find`

Scan the repository and generate hotspot reports

**Usage**

```text
Usage: find [OPTIONS]
```

**Machine contract:** schema 5 report (`git slop schema report`); `find --estimate-only` uses `find-estimate-1`.

| Argument | Value | Default | Constraints | Description |
| --- | --- | --- | --- | --- |
| `--allow-shallow` | `flag` | `-` | - | Acknowledge incomplete history and continue in a shallow clone |
| `--scope` | `SCOPE` | `-` | - | Analyze only this repo-relative path while retaining repository-wide Git evidence |
| `--allow-empty-scope` | `flag` | `-` | - | Permit a scope that selects no tracked paths and emit an empty analysis |
| `--quiet` | `flag` | `-` | - | Suppress human progress and report-path messages |
| `--no-progress` | `flag` | `-` | - | Suppress phase progress while preserving the final result |
| `--state-dir` | `PATH` | `-` | - | Mutable cache/state directory. Relative paths resolve from the repository root |
| `--output-dir` | `PATH` | `-` | - | Report output directory. Relative paths resolve from the repository root |
| `--no-cache` | `flag` | `-` | - | Disable token-cache reads and writes for an ephemeral scan |
| `--ephemeral` | `flag` | `-` | conflicts: --output-dir, --state-dir | Keep disposable state and reports under Git-private storage, without adopting `.slop/` |
| `--allow-degraded` | `flag` | `-` | - | Deterministically analyze the largest path prefix that fits the memory budget |
| `--as-of` | `RFC3339` | `-` | - | Fixed RFC 3339 analysis clock for reproducible recency and history windows |
| `--report-profile` | `REPORT_PROFILE` | `standard` | values: compact, standard, full-evidence | Report evidence profile |
| `--compression` | `COMPRESSION` | `none` | values: none, gzip, zstd | Also write a compressed report beside report.json |
| `--estimate-only` | `flag` | `-` | - | Estimate scope, memory, cache, report size, time, and inodes without scanning |

**Example**

```sh
git slop find --ephemeral
```
