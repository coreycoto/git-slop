# Git Slop CLI Reference: `doctor`

Generated from the live Clap command tree.

## `git-slop doctor`

Diagnose repository readiness and optionally write a redacted bundle

**Usage**

```text
Usage: doctor [OPTIONS]
```

**Machine contract:** `doctor-1`.

| Argument | Value | Default | Constraints | Description |
| --- | --- | --- | --- | --- |
| `--bundle` | `BUNDLE` | `-` | - | Write a privacy-safe diagnostic JSON bundle |
| `--format` | `FORMAT` | `text` | values: text, json | Output format |
| `--scope` | `SCOPE` | `-` | - | Estimate only this repo-relative scope |
| `--require-current` | `flag` | `-` | - | Return exit 2 when the latest report is valid but stale |

**Example**

```sh
git slop doctor --require-current
```
