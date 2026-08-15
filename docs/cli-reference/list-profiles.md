# Git Slop CLI Reference: `list profiles`

Generated from the live Clap command tree.

## `git-slop list profiles`

List aggregate analysis-profile totals

**Usage**

```text
Usage: git-slop list profiles [OPTIONS]
```

**Machine contract:** `list-1`.

| Argument | Value | Default | Constraints | Description |
| --- | --- | --- | --- | --- |
| `--report` | `REPORT` | `-` | - | Report path; defaults to the latest durable or Git-private report |
| `--require-current` | `flag` | `-` | - | Fail unless the report matches current repository state |
| `--top` | `TOP` | `50` | - | Maximum returned records |
| `--format` | `FORMAT` | `text` | values: text, json, yaml | Output format |
| `--wide` | `flag` | `-` | - | Use a wider terminal layout |
| `--no-truncate` | `flag` | `-` | - | Do not truncate terminal fields |
| `--profile` | `PROFILE` | `-` | - | Match an analysis profile |

**Example**

```sh
git slop list profiles --format json
```
