# Git Slop CLI Reference: `compare`

Generated from the live Clap command tree.

## `git-slop compare`

Compare two existing schema-5 reports without rerunning the detector

**Usage**

```text
Usage: git-slop compare [OPTIONS]
```

**Machine contract:** `compare-1`; NDJSON streaming uses `compare-ndjson-1`.

| Argument | Value | Default | Constraints | Description |
| --- | --- | --- | --- | --- |
| `--state-dir` | `PATH` | `-` | - | Mutable state directory for named baselines. Relative paths resolve from the repository root |
| `--base` | `BASE` | `-` | conflicts: --base-ref, --baseline | Base report.json path |
| `--base-ref` | `BASE_REF` | `-` | conflicts: --base, --baseline | Safely resolve and scan this Git revision in an isolated worktree |
| `--baseline` | `BASELINE` | `-` | conflicts: --base, --base-ref | Use a named baseline from Git-private runtime storage |
| `--head` | `HEAD` | `.slop/latest/report.json` | - | Head report.json path |
| `--scope` | `SCOPE` | `-` | - | Apply the head repository's scope to an isolated --base-ref scan |
| `--allow-shallow` | `flag` | `-` | - | Permit incomplete history in an isolated --base-ref scan |
| `--allow-incomplete-evidence` | `flag` | `-` | - | Permit comparison when selected inventory records are incomplete |
| `--top` | `TOP` | `10` | - | Maximum number of changed files and queue movements to show |
| `--format` | `FORMAT` | `text` | values: text, json, yaml, ndjson | Output format |
| `--detail` | `DETAIL` | `top` | values: summary, top, full | Detail level for machine output |
| `--offset` | `OFFSET` | `0` | - | Zero-based record offset for --detail full |
| `--limit` | `LIMIT` | `1000` | - | Maximum records per collection for --detail full |
| `--force` | `flag` | `-` | - | Compare reports with incompatible identity or analyzer metadata |
| `--include-local-paths` | `flag` | `-` | - | Include local filesystem report paths in output descriptors |
| `--include-unchanged` | `flag` | `-` | - | Include unchanged file and folder records in bounded compare collections |
| `--policy-from` | `POLICY_FROM` | `base` | values: base, head | Select which report supplies regression thresholds and evidence-drift policy |
| `--fail-on-regression` | `flag` | `-` | - | Exit 1 when an existing file worsens or a newly added file is a finding |

**Example**

```sh
git slop compare --baseline main --fail-on-regression
```
