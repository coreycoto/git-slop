# Git Slop CLI Reference: `show`

Generated from the live Clap command tree.

## `git-slop show`

Show metrics and reasons for one file or folder

**Usage**

```text
Usage: git-slop show [OPTIONS] <TARGET_PATH>
```

**Machine contract:** `show-1`.

| Argument | Value | Default | Constraints | Description |
| --- | --- | --- | --- | --- |
| `target_path` | `TARGET_PATH` | `-` | required | Repo-relative file or folder path |
| `--report` | `REPORT` | `-` | - | Report path. Defaults to the durable latest report, then the Git-private first-run report |
| `--require-current` | `flag` | `-` | - | Fail when the report does not match current HEAD, worktree, config, scope, or analyzer |
| `--format` | `FORMAT` | `text` | values: text, json, yaml | Output format |

**Example**

```sh
git slop show src/lib.rs
```
