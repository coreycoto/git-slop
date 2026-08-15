# Git Slop CLI Reference: `list interventions`

Generated from the live Clap command tree.

## `git-slop list interventions`

List maintenance candidates that warrant review

**Usage**

```text
Usage: git-slop list interventions [OPTIONS]
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
| `--path` | `PATH` | `-` | - | Match a finding path |
| `--profile` | `PROFILE` | `-` | - | Match an analysis profile |
| `--language` | `LANGUAGE` | `-` | - | Match a resolved file language |
| `--classification` | `CLASSIFICATION` | `-` | - | Match a resolved file classification |
| `--severity` | `SEVERITY` | `-` | - | Match review severity |
| `--context-band` | `CONTEXT_BAND` | `-` | - | Match context/load band |
| `--slop-band` | `SLOP_BAND` | `-` | - | Match maintenance-pressure band |

**Example**

```sh
git slop list interventions --top 20
```
