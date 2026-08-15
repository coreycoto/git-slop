# Git Slop CLI Reference: `reference`

Generated from the live Clap command tree.

## `git-slop reference`

Generate Markdown command reference from the live Clap command tree

**Usage**

```text
Usage: git-slop reference [OPTIONS]
```

| Argument | Value | Default | Constraints | Description |
| --- | --- | --- | --- | --- |
| `--output` | `OUTPUT` | `-` | - | Index destination. Detailed command pages use the sibling stem directory. Without an output, the complete reference is written to stdout |

**Example**

```sh
git slop reference --output docs/cli-reference.md
```
