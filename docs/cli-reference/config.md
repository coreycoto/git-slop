# Git Slop CLI Reference: `config`

Generated from the live Clap command tree.

## `git-slop config`

Inspect or migrate effective configuration

**Usage**

```text
Usage: config <COMMAND>
```

**Machine contract:** `config-2`.

**Example**

```sh
git slop config show --effective
```

## `git-slop config show`

Show configuration; --effective includes defaults

**Usage**

```text
Usage: show [OPTIONS]
```

**Machine contract:** `config-2`.

| Argument | Value | Default | Constraints | Description |
| --- | --- | --- | --- | --- |
| `--effective` | `flag` | `-` | - | Include defaults after applying repository overrides |

**Example**

```sh
git slop config show --effective
```

## `git-slop config validate`

Validate the local configuration

**Usage**

```text
Usage: validate
```

**Machine contract:** `config-2`.

**Example**

```sh
git slop config validate
```

## `git-slop config diff-defaults`

Show only values that differ from defaults

**Usage**

```text
Usage: diff-defaults
```

**Machine contract:** `config-2`.

**Example**

```sh
git slop config diff-defaults
```

## `git-slop config migrate`

Rewrite legacy schema configuration as a minimal schema-2 override

**Usage**

```text
Usage: migrate [OPTIONS]
```

**Machine contract:** `config-2`.

| Argument | Value | Default | Constraints | Description |
| --- | --- | --- | --- | --- |
| `--dry-run` | `flag` | `-` | - | Print the migrated configuration without writing it |
| `--no-backup` | `flag` | `-` | - | Do not retain the existing configuration as config.yaml.bak |

**Example**

```sh
git slop config migrate --dry-run
```

## `git-slop config schema`

Print the supported configuration schema as JSON

**Usage**

```text
Usage: schema
```

**Machine contract:** `config-2`.

**Example**

```sh
git slop config schema
```
