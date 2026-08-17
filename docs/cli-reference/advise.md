# Git Slop CLI Reference: `advise`

Generated from the live Clap command tree.

## `git-slop advise`

Build provider-free policy context or validate an existing advice artifact

**Usage**

```text
Usage: git-slop advise [OPTIONS]
```

**Machine contract:** `advice-input-1` for provider-free context; `advice-1` for release-gated validated advice.

| Argument | Value | Default | Constraints | Description |
| --- | --- | --- | --- | --- |
| `--report` | `REPORT` | `-` | - | Report path. Advice always requires this report to match the current worktree |
| `--path` | `PATH` | `-` | conflicts: --validate-artifact; exclusive group: path, cluster, relationship, top | Repo-relative file or folder path |
| `--relationship` | `RELATIONSHIP` | `-` | conflicts: --validate-artifact; exclusive group: path, cluster, relationship, top | Relationship identifier |
| `--cluster` | `CLUSTER` | `-` | conflicts: --validate-artifact; exclusive group: path, cluster, relationship, top | Cluster identifier |
| `--top` | `TOP` | `-` | conflicts: --validate-artifact; exclusive group: path, cluster, relationship, top | Evaluate the top N deterministic interventions, then health refactor candidates |
| `--policy` | `POLICIES` | `-` | conflicts: --validate-artifact | Evaluate only this already-locked pack or rule in addition to all core invariants |
| `--context-only` | `flag` | `-` | conflicts: --validate-artifact | Emit provider-independent context without model inference; defaults to full JSON |
| `--ephemeral` | `flag` | `-` | conflicts: --validate-artifact | Avoid context-cache and advice-state writes; useful for disposable benchmarks |
| `--validate-artifact` | `VALIDATE_ARTIFACT` | `-` | - | Validate and render an existing advice artifact against the current selected report |
| `--max-context-bytes` | `MAX_CONTEXT_BYTES` | `131072` | conflicts: --validate-artifact | Maximum provider-independent context size in bytes |
| `--max-context-tokens` | `MAX_CONTEXT_TOKENS` | `8192` | conflicts: --validate-artifact | Maximum estimated o200k_harmony input tokens |
| `--excerpt-bytes` | `EXCERPT_BYTES` | `4096` | conflicts: --validate-artifact | Maximum bytes included from each repository file |
| `--max-slices` | `MAX_SLICES` | `3` | conflicts: --validate-artifact | Maximum plan slices generated for one non-top selector |
| `--format` | `FORMAT` | `-` | values: markdown, json | Render provider-free context or validated advice as Markdown/JSON |
| `--output` | `OUTPUT` | `-` | - | Also write the selected rendering to this repo-relative or absolute path |

**Example**

```sh
git slop advise --top 1
```
