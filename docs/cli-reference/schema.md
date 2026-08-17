# Git Slop CLI Reference: `schema`

Generated from the live Clap command tree.

## `git-slop schema`

Print a published JSON Schema for a machine contract

**Usage**

```text
Usage: git-slop schema [OPTIONS] <CONTRACT>
```

**Machine contract:** the selected immutable schema.

| Argument | Value | Default | Constraints | Description |
| --- | --- | --- | --- | --- |
| `contract` | `CONTRACT` | `-` | required; values: report, config, compare, explain, plan, sarif, health, check, doctor, build-info, release-manifest, list, show, prompt-manifest, error, find-estimate, cache-status, cache-prune, baseline, prune, compare-ndjson, policy-pack, policy-lock, advice-input, advice-response, advice, advisor-corpus, advisor-ratings, advisor-ratings-v2, advisor-review-artifact, advisor-review-manifest, advisor-operation-receipt, advisor-thresholds, advisor-benchmark, advisor-capacity | Machine contract whose immutable schema should be printed |
| `--output` | `OUTPUT` | `-` | - | Destination file. Defaults to stdout |

**Example**

```sh
git slop schema report
```
