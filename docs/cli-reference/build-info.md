# Git Slop CLI Reference: `build-info`

Generated from the live Clap command tree.

## `git-slop build-info`

Print package and source-build provenance

**Usage**

```text
Usage: git-slop build-info [OPTIONS]
```

**Machine contract:** `build-info-2`.

| Argument | Value | Default | Constraints | Description |
| --- | --- | --- | --- | --- |
| `--format` | `FORMAT` | `json` | values: json | Machine-readable build provenance format |

**Example**

```sh
git slop build-info --format json
```
