# Git Slop CLI Reference: `prune`

Generated from the live Clap command tree.

## `git-slop prune`

Remove old immutable run snapshots according to retention policy

**Usage**

```text
Usage: git-slop prune [OPTIONS]
```

**Machine contract:** `prune-1`.

| Argument | Value | Default | Constraints | Description |
| --- | --- | --- | --- | --- |
| `--state-dir` | `PATH` | `-` | - | Mutable state directory. Defaults to the same active root as find |
| `--keep` | `KEEP` | `-` | - | Number of newest run snapshots to retain; defaults to output.retention_runs |
| `--max-bytes` | `MAX_BYTES` | `-` | - | Maximum total bytes retained; defaults to output.retention_bytes |
| `--dry-run` | `flag` | `-` | conflicts: --yes | Explicitly request preview behavior (preview is already the default) |
| `--yes` | `flag` | `-` | conflicts: --dry-run | Apply the selected removals. Without this flag the command is read-only |
| `--format` | `FORMAT` | `text` | values: text, json, yaml | Select text, JSON, or YAML output |

**Example**

```sh
git slop prune --keep 20 --yes
```
