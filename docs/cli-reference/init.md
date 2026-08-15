# Git Slop CLI Reference: `init`

Generated from the live Clap command tree.

## `git-slop init`

Scaffold .slop/ config, ignore rules, and state directories

**Usage**

```text
Usage: git-slop init [OPTIONS]
```

| Argument | Value | Default | Constraints | Description |
| --- | --- | --- | --- | --- |
| `--force` | `flag` | `-` | exclusive group: force, repair, check | Replace generated files atomically and keep ignored `.bak` recovery copies |
| `--repair` | `flag` | `-` | exclusive group: force, repair, check | Add missing generated ignore rules without replacing repository configuration |
| `--check` | `flag` | `-` | exclusive group: force, repair, check | Inspect adoption files without changing the repository |
| `--gitignore-only` | `flag` | `-` | - | Limit initialization, repair, force, or check to .slop/.gitignore |

**Example**

```sh
git slop init --check
```
