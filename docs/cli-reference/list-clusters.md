# Git Slop CLI Reference: `list clusters`

Generated from the live Clap command tree.

## `git-slop list clusters`

List structural or consolidation clusters

**Usage**

```text
Usage: git-slop list clusters [OPTIONS]
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
| `--path` | `PATH` | `-` | - | Match a cluster member path |
| `--profile` | `PROFILE` | `-` | - | Match a member analysis profile |
| `--language` | `LANGUAGE` | `-` | - | Match a member file language |
| `--classification` | `CLASSIFICATION` | `-` | - | Match a member file classification |

**Example**

```sh
git slop list clusters --top 20
```
