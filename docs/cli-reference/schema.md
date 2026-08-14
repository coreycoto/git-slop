# Git Slop CLI Reference: `schema`

Generated from the live Clap command tree.

## `git-slop schema`

Print a published JSON Schema for a machine contract

**Usage**

```text
Usage: schema [OPTIONS] <CONTRACT>
```

**Machine contract:** the selected immutable schema.

| Argument | Value | Default | Constraints | Description |
| --- | --- | --- | --- | --- |
| `contract` | `CONTRACT` | `-` | required; values: report, config, compare, explain, plan, sarif, health, check, doctor, build-info, release-manifest, list, show, prompt-manifest, error, find-estimate, cache-status, cache-prune, baseline, prune, compare-ndjson | Machine contract whose immutable schema should be printed |
| `--output` | `OUTPUT` | `-` | - | Destination file. Defaults to stdout |

**Example**

```sh
git slop schema report
```
