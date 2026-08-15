# Git Slop CLI Reference: `cache`

Generated from the live Clap command tree.

## `git-slop cache`

Inspect or prune the packed token cache

**Usage**

```text
Usage: git-slop cache [OPTIONS] <COMMAND>
```

| Argument | Value | Default | Constraints | Description |
| --- | --- | --- | --- | --- |
| `--state-dir` | `PATH` | `-` | global | Mutable state directory. Defaults to .slop, matching find |

**Example**

```sh
git slop cache status
```

## `git-slop cache status`

Show packed-cache location, entry count, and logical size

**Usage**

```text
Usage: git-slop cache status [OPTIONS]
```

**Machine contract:** `cache-status-1`.

| Argument | Value | Default | Constraints | Description |
| --- | --- | --- | --- | --- |
| `--format` | `FORMAT` | `text` | values: text, json, yaml | Output format |

**Example**

```sh
git slop cache status --format json
```

## `git-slop cache prune`

Preview or apply bounded packed-cache retention

**Usage**

```text
Usage: git-slop cache prune [OPTIONS]
```

**Machine contract:** `cache-prune-1`.

| Argument | Value | Default | Constraints | Description |
| --- | --- | --- | --- | --- |
| `--max-entries` | `MAX_ENTRIES` | `10000` | - | Maximum entries to retain |
| `--max-bytes` | `MAX_BYTES` | `536870912` | - | Maximum logical payload bytes to retain |
| `--dry-run` | `flag` | `-` | conflicts: --yes | Explicitly request preview behavior (preview is already the default) |
| `--yes` | `flag` | `-` | conflicts: --dry-run | Apply the selected removals. Without this flag the command is read-only |
| `--compact` | `flag` | `-` | - | Reclaim free database pages after pruning |
| `--format` | `FORMAT` | `text` | values: text, json, yaml | Output format |

**Example**

```sh
git slop cache prune --dry-run
```
