# Git Slop CLI Reference: `baseline`

Generated from the live Clap command tree.

## `git-slop baseline`

Manage named comparison baselines in Git-private runtime storage

**Usage**

```text
Usage: git-slop baseline [OPTIONS] <COMMAND>
```

**Machine contract:** `baseline-1`.

| Argument | Value | Default | Constraints | Description |
| --- | --- | --- | --- | --- |
| `--state-dir` | `PATH` | `-` | global | Mutable state directory. Relative paths resolve from the repository root |

**Example**

```sh
git slop baseline list
```

## `git-slop baseline ensure`

Idempotently save a named baseline, failing closed when stored content differs

**Usage**

```text
Usage: git-slop baseline ensure [OPTIONS]
```

**Machine contract:** `baseline-1`.

| Argument | Value | Default | Constraints | Description |
| --- | --- | --- | --- | --- |
| `--name` | `NAME` | `default` | - | Stable baseline name |
| `--report` | `REPORT` | `-` | - | Report path. Defaults to the durable latest report, then the Git-private first-run report |
| `--require-current` | `flag` | `-` | - | Fail when the report does not match current HEAD, worktree, config, scope, or analyzer |
| `--replace` | `flag` | `-` | - | Explicitly replace a differing stored baseline |
| `--allow-dirty` | `flag` | `-` | - | Permit a report produced from a dirty worktree |
| `--allow-incomplete-evidence` | `flag` | `-` | - | Permit incomplete inventory or history evidence |
| `--format` | `FORMAT` | `text` | values: text, json, yaml | Output format |

**Example**

```sh
git slop baseline ensure --name main
```

## `git-slop baseline create`

Create a named baseline from a validated report

**Usage**

```text
Usage: git-slop baseline create [OPTIONS]
```

**Machine contract:** `baseline-1`.

| Argument | Value | Default | Constraints | Description |
| --- | --- | --- | --- | --- |
| `--name` | `NAME` | `default` | - | Stable baseline name |
| `--report` | `REPORT` | `-` | - | Report path. Defaults to the durable latest report, then the Git-private first-run report |
| `--require-current` | `flag` | `-` | - | Fail when the report does not match current HEAD, worktree, config, scope, or analyzer |
| `--force` | `flag` | `-` | - | Replace an existing named baseline |
| `--allow-dirty` | `flag` | `-` | - | Permit a report produced from a dirty worktree |
| `--allow-incomplete-evidence` | `flag` | `-` | - | Permit incomplete inventory or history evidence |
| `--format` | `FORMAT` | `text` | values: text, json, yaml | Output format |

**Example**

```sh
git slop baseline create --name main
```

## `git-slop baseline update`

Replace an existing named baseline from a validated report

**Usage**

```text
Usage: git-slop baseline update [OPTIONS]
```

**Machine contract:** `baseline-1`.

| Argument | Value | Default | Constraints | Description |
| --- | --- | --- | --- | --- |
| `--name` | `NAME` | `default` | - | Stable baseline name |
| `--report` | `REPORT` | `-` | - | Report path. Defaults to the durable latest report, then the Git-private first-run report |
| `--require-current` | `flag` | `-` | - | Fail when the report does not match current HEAD, worktree, config, scope, or analyzer |
| `--allow-dirty` | `flag` | `-` | - | Permit a report produced from a dirty worktree |
| `--allow-incomplete-evidence` | `flag` | `-` | - | Permit incomplete inventory or history evidence |
| `--format` | `FORMAT` | `text` | values: text, json, yaml | Output format |

**Example**

```sh
git slop baseline update --name main
```

## `git-slop baseline list`

List named baselines with identity and readiness metadata

**Usage**

```text
Usage: git-slop baseline list [OPTIONS]
```

**Machine contract:** `baseline-1`.

| Argument | Value | Default | Constraints | Description |
| --- | --- | --- | --- | --- |
| `--format` | `FORMAT` | `text` | values: text, json, yaml | Output format |

**Example**

```sh
git slop baseline list --format json
```

## `git-slop baseline inspect`

Inspect baseline identity and evidence status

**Usage**

```text
Usage: git-slop baseline inspect [OPTIONS]
```

**Machine contract:** `baseline-1`.

| Argument | Value | Default | Constraints | Description |
| --- | --- | --- | --- | --- |
| `--name` | `NAME` | `default` | - | Stable baseline name |
| `--format` | `FORMAT` | `text` | values: text, json, yaml | Output format |

**Example**

```sh
git slop baseline inspect --name main
```

## `git-slop baseline validate`

Validate a named baseline against the current report contract

**Usage**

```text
Usage: git-slop baseline validate [OPTIONS]
```

**Machine contract:** `baseline-1`.

| Argument | Value | Default | Constraints | Description |
| --- | --- | --- | --- | --- |
| `--name` | `NAME` | `default` | - | Stable baseline name |
| `--format` | `FORMAT` | `text` | values: text, json, yaml | Output format |

**Example**

```sh
git slop baseline validate --name main
```

## `git-slop baseline remove`

Remove a named baseline

**Usage**

```text
Usage: git-slop baseline remove [OPTIONS]
```

**Machine contract:** `baseline-1`.

| Argument | Value | Default | Constraints | Description |
| --- | --- | --- | --- | --- |
| `--name` | `NAME` | `default` | - | Stable baseline name |
| `--format` | `FORMAT` | `text` | values: text, json, yaml | Output format |
| `--yes` | `flag` | `-` | - | Apply the removal. Without this flag the command is a read-only preview |

**Example**

```sh
git slop baseline remove --name main --yes
```
