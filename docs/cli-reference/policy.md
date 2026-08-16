# Git Slop CLI Reference: `policy`

Generated from the live Clap command tree.

## `git-slop policy`

Manage declarative policy packs used only by the optional advisor

**Usage**

```text
Usage: git-slop policy <COMMAND>
```

**Example**

```sh
git slop policy list
```

## `git-slop policy init`

Create a minimal data-only policy pack

**Usage**

```text
Usage: git-slop policy init [OPTIONS] <DIRECTORY>
```

| Argument | Value | Default | Constraints | Description |
| --- | --- | --- | --- | --- |
| `directory` | `DIRECTORY` | `-` | required | Empty directory to populate. Relative paths resolve from the repository root |
| `--format` | `FORMAT` | `text` | values: text, json | Output format |

**Example**

```sh
git slop policy init ./team-policy
```

## `git-slop policy validate`

Validate a local directory, installed pack ID, or the built-in core pack

**Usage**

```text
Usage: git-slop policy validate [OPTIONS] <TARGET>
```

| Argument | Value | Default | Constraints | Description |
| --- | --- | --- | --- | --- |
| `target` | `TARGET` | `-` | required | Policy-pack directory or installed pack ID |
| `--format` | `FORMAT` | `text` | values: text, json | Output format |

**Example**

```sh
git slop policy validate ./team-policy
```

## `git-slop policy test`

Run static golden cases for a local or installed data-only pack

**Usage**

```text
Usage: git-slop policy test [OPTIONS] <TARGET>
```

| Argument | Value | Default | Constraints | Description |
| --- | --- | --- | --- | --- |
| `target` | `TARGET` | `-` | required | Policy-pack directory or installed pack ID |
| `--format` | `FORMAT` | `text` | values: text, json | Output format |

**Example**

```sh
git slop policy test ./team-policy
```

## `git-slop policy install`

Explicitly copy a validated local pack into the user policy cache

**Usage**

```text
Usage: git-slop policy install [OPTIONS] <SOURCE>
```

| Argument | Value | Default | Constraints | Description |
| --- | --- | --- | --- | --- |
| `source` | `SOURCE` | `-` | required | Local policy-pack directory. Network acquisition is not implicit in v1 |
| `--select` | `flag` | `-` | - | Add this pack to .slop/policies.yaml after installation |
| `--format` | `FORMAT` | `text` | values: text, json | Output format |

**Example**

```sh
git slop policy install ./team-policy --select
```

## `git-slop policy lock`

Resolve selected packs and write .slop/policy-lock.json

**Usage**

```text
Usage: git-slop policy lock [OPTIONS]
```

**Machine contract:** `policy-lock-1`.

| Argument | Value | Default | Constraints | Description |
| --- | --- | --- | --- | --- |
| `--format` | `FORMAT` | `text` | values: text, json | Output format |

**Example**

```sh
git slop policy lock --format json
```

## `git-slop policy list`

List the built-in and user-installed policy packs

**Usage**

```text
Usage: git-slop policy list [OPTIONS]
```

| Argument | Value | Default | Constraints | Description |
| --- | --- | --- | --- | --- |
| `--format` | `FORMAT` | `text` | values: text, json | Output format |

**Example**

```sh
git slop policy list --format json
```

## `git-slop policy show`

Inspect a complete pack or one stable rule ID

**Usage**

```text
Usage: git-slop policy show [OPTIONS] <TARGET>
```

| Argument | Value | Default | Constraints | Description |
| --- | --- | --- | --- | --- |
| `target` | `TARGET` | `-` | required | Installed pack ID, rule ID, core, or local pack directory |
| `--format` | `FORMAT` | `text` | values: text, json | Output format |

**Example**

```sh
git slop policy show core --format json
```

## `git-slop policy remove`

Remove an installed third-party pack from the user cache

**Usage**

```text
Usage: git-slop policy remove [OPTIONS] <PACK_ID>
```

| Argument | Value | Default | Constraints | Description |
| --- | --- | --- | --- | --- |
| `pack_id` | `PACK_ID` | `-` | required | Installed policy-pack ID |
| `--unselect` | `flag` | `-` | - | Remove the pack from .slop/policies.yaml and invalidate its lock first |
| `--format` | `FORMAT` | `text` | values: text, json | Output format |

**Example**

```sh
git slop policy remove com.example.team-policy --unselect
```
