# Git Slop CLI Reference: `explain`

Generated from the live Clap command tree.

## `git-slop explain`

Explain why selected hotspots or structural findings are expensive

**Usage**

```text
Usage: explain [OPTIONS]
```

**Machine contract:** `explain-2`.

| Argument | Value | Default | Constraints | Description |
| --- | --- | --- | --- | --- |
| `--report` | `REPORT` | `-` | - | Report path. Defaults to .slop/latest/report.json |
| `--path` | `PATH` | `-` | exclusive group: path, cluster, relationship, top | Repo-relative file or folder path |
| `--cluster` | `CLUSTER` | `-` | exclusive group: path, cluster, relationship, top | Cluster identifier |
| `--relationship` | `RELATIONSHIP` | `-` | exclusive group: path, cluster, relationship, top | Relationship identifier |
| `--top` | `TOP` | `-` | exclusive group: path, cluster, relationship, top | Explain the top N hotspots from the action queue |
| `--format` | `FORMAT` | `text` | values: text, json, yaml | Output format |
| `--prompt-pack` | `PROMPT_PACK` | `-` | - | Write a deterministic local-model prompt pack to this directory |
| `--force` | `flag` | `-` | - | Atomically replace an existing prompt-pack directory |
| `--include-repository-context` | `flag` | `-` | - | Include bounded local source/test excerpts, guidance, and verification hints |
| `--excerpt-bytes` | `EXCERPT_BYTES` | `2048` | - | Maximum bytes read from each included repository file |
| `--include-local-paths` | `flag` | `-` | - | Include local filesystem paths in prompt-pack provenance and commands |

**Example**

```sh
git slop explain --path src/lib.rs
```
