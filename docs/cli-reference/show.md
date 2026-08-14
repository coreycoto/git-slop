# Git Slop CLI Reference: `show`

Generated from the live Clap command tree.

## `git-slop show`

Show metrics and reasons for one file or folder

**Usage**

```text
Usage: show [OPTIONS] <TARGET_PATH>
```

**Machine contract:** `show-1`.

| Argument | Value | Default | Constraints | Description |
| --- | --- | --- | --- | --- |
| `target_path` | `TARGET_PATH` | `-` | required | Repo-relative file or folder path |
| `--report` | `REPORT` | `-` | - | Report path. Defaults to .slop/latest/report.json |
| `--format` | `FORMAT` | `text` | values: text, json, yaml | Output format |

**Example**

```sh
git slop show src/lib.rs
```
