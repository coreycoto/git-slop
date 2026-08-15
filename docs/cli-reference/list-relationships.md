# Git Slop CLI Reference: `list relationships`

Generated from the live Clap command tree.

## `git-slop list relationships`

List evidence-backed relationships between paths

**Usage**

```text
Usage: git-slop list relationships [OPTIONS]
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
| `--path` | `PATH` | `-` | - | Match a relationship endpoint |
| `--profile` | `PROFILE` | `-` | - | Match an endpoint analysis profile |
| `--language` | `LANGUAGE` | `-` | - | Match an endpoint file language |
| `--classification` | `CLASSIFICATION` | `-` | - | Match an endpoint file classification |

**Example**

```sh
git slop list relationships --top 20
```
