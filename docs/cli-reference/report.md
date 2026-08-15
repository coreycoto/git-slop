# Git Slop CLI Reference: `report`

Generated from the live Clap command tree.

## `git-slop report`

Validate or inspect the versioned report contract

**Usage**

```text
Usage: git-slop report <COMMAND>
```

**Machine contract:** schema 5 report (`git slop schema report`).

**Example**

```sh
git slop report validate .slop/latest/report.json
```

## `git-slop report validate`

Validate one report against the complete schema-5 contract

**Usage**

```text
Usage: git-slop report validate [OPTIONS] [REPORT_JSON]
```

**Machine contract:** schema 5 report (`git slop schema report`).

| Argument | Value | Default | Constraints | Description |
| --- | --- | --- | --- | --- |
| `path` | `REPORT_JSON` | `-` | - | Report JSON to validate |
| `--report` | `REPORT_JSON` | `-` | - | Report JSON to validate (alias for the positional path) |
| `--allow-legacy` | `flag` | `-` | - | Accept schema 4 as migration input and validate its normalized schema-5 form |
| `--format` | `FORMAT` | `text` | values: text, json, yaml | Success output format |

**Example**

```sh
git slop report validate .slop/latest/report.json
```

## `git-slop report migrate`

Migrate a schema-4 report to normalized schema 5

**Usage**

```text
Usage: git-slop report migrate --output <PATH> <REPORT_JSON>
```

**Machine contract:** schema 5 report (`git slop schema report`).

| Argument | Value | Default | Constraints | Description |
| --- | --- | --- | --- | --- |
| `path` | `REPORT_JSON` | `-` | required | Legacy report to migrate |
| `--output` | `PATH` | `-` | required | Destination for the normalized schema-5 report |

**Example**

```sh
git slop report migrate old.json --output report.json
```

## `git-slop report schema`

Print the published JSON Schema for report schema 5

**Usage**

```text
Usage: git-slop report schema
```

**Machine contract:** schema 5 report (`git slop schema report`).

**Example**

```sh
git slop report schema
```
