# Git Slop CLI Reference: `sarif`

Generated from the live Clap command tree.

## `git-slop sarif`

Export action-queue findings from an existing schema-5 report as SARIF

**Usage**

```text
Usage: git-slop sarif [OPTIONS]
```

**Machine contract:** `sarif-1`.

| Argument | Value | Default | Constraints | Description |
| --- | --- | --- | --- | --- |
| `--report` | `REPORT` | `-` | - | Report path. Defaults to the durable latest report, then the Git-private first-run report |
| `--require-current` | `flag` | `-` | - | Fail when the report does not match current HEAD, worktree, config, scope, or analyzer |
| `--top` | `TOP` | `-` | - | Maximum number of action-queue findings to export |
| `--scope` | `SCOPE` | `action-queue` | values: policy, action-queue | Export configured policy failures or action-queue intervention candidates |
| `--output` | `OUTPUT` | `-` | - | Optional SARIF output path. Defaults to stdout |
| `--include-local-paths` | `flag` | `-` | - | Include the local source report path in SARIF invocation properties |

**Example**

```sh
git slop sarif --output .slop/latest/findings.sarif
```
