# Git Slop CLI Reference: `check`

Generated from the live Clap command tree.

## `git-slop check`

Evaluate an existing report against CI thresholds

**Usage**

```text
Usage: git-slop check [OPTIONS]
```

**Machine contract:** `check-1`.

| Argument | Value | Default | Constraints | Description |
| --- | --- | --- | --- | --- |
| `--report` | `REPORT` | `-` | - | Report path. Defaults to the durable latest report, then the Git-private first-run report |
| `--fail-on-context-band` | `FAIL_ON_CONTEXT_BAND` | `-` | values: compact, healthy, warning, critical | Override the config default fail threshold for context_band |
| `--fail-on-slop-band` | `FAIL_ON_SLOP_BAND` | `-` | values: low, moderate, high, critical | Override the config default fail threshold for slop_band |
| `--format` | `FORMAT` | `text` | values: text, json, github | Output format, including escaped GitHub workflow commands |
| `--details` | `flag` | `-` | - | Include complete finding records in JSON output |
| `--include-folders` | `flag` | `-` | - | Include folder records in addition to the versioned file-only gate |
| `--offset` | `OFFSET` | `0` | - | Zero-based finding offset used with --details |
| `--limit` | `LIMIT` | `1000` | - | Maximum finding records returned with --details |
| `--allow-incomplete-evidence` | `flag` | `-` | - | Permit policy evaluation when selected inventory records are incomplete |
| `--evaluate-only` | `flag` | `-` | - | Evaluate and report the canonical policy result without returning exit 1 for findings |
| `--require-current` | `flag` | `-` | - | Fail when the report does not match current HEAD, worktree, config, scope, or analyzer |

**Example**

```sh
git slop check --require-current
```
