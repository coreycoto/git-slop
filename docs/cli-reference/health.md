# Git Slop CLI Reference: `health`

Generated from the live Clap command tree.

## `git-slop health`

Render repository health for CI summaries, annotations, or automation

**Usage**

```text
Usage: health [OPTIONS]
```

**Machine contract:** `health-1` for JSON output.

| Argument | Value | Default | Constraints | Description |
| --- | --- | --- | --- | --- |
| `--report` | `REPORT` | `-` | - | Report path. Defaults to .slop/latest/report.json |
| `--format` | `FORMAT` | `text` | values: text, markdown, github, json | Output suited for a job summary, workflow annotations, or automation |
| `--max-annotations` | `MAX_ANNOTATIONS` | `10` | - | Maximum number of GitHub workflow annotations to emit |
| `--require-current` | `flag` | `-` | - | Fail when the report does not match current HEAD, worktree, config, scope, or analyzer |

**Example**

```sh
git slop health --format markdown --require-current
```
