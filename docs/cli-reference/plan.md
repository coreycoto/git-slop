# Git Slop CLI Reference: `plan`

Generated from the live Clap command tree.

## `git-slop plan`

Propose bounded maintenance slices from the current detector report

**Usage**

```text
Usage: plan [OPTIONS] <--path <PATH>|--cluster <CLUSTER>|--relationship <RELATIONSHIP>>
```

**Machine contract:** `plan-2`.

| Argument | Value | Default | Constraints | Description |
| --- | --- | --- | --- | --- |
| `--report` | `REPORT` | `-` | - | Report path. Defaults to .slop/latest/report.json |
| `--path` | `PATH` | `-` | exclusive group: path, cluster, relationship; one required from: path, cluster, relationship | Repo-relative file or folder path |
| `--cluster` | `CLUSTER` | `-` | exclusive group: path, cluster, relationship; one required from: path, cluster, relationship | Cluster identifier |
| `--relationship` | `RELATIONSHIP` | `-` | exclusive group: path, cluster, relationship; one required from: path, cluster, relationship | Relationship identifier |
| `--max-slices` | `MAX_SLICES` | `3` | - | Maximum number of bounded maintenance slices to propose |
| `--format` | `FORMAT` | `text` | values: text, json, yaml | Output format |
| `--prompt-pack` | `PROMPT_PACK` | `-` | - | Write a deterministic local-model prompt pack to this directory |
| `--force` | `flag` | `-` | - | Atomically replace an existing prompt-pack directory |
| `--include-repository-context` | `flag` | `-` | - | Include bounded local source/test excerpts, guidance, and verification hints |
| `--excerpt-bytes` | `EXCERPT_BYTES` | `2048` | - | Maximum bytes read from each included repository file |
| `--include-local-paths` | `flag` | `-` | - | Include local filesystem paths in prompt-pack provenance and commands |

**Example**

```sh
git slop plan --path src/lib.rs
```
