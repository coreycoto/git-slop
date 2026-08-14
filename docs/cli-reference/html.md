# Git Slop CLI Reference: `html`

Generated from the live Clap command tree.

## `git-slop html`

Write a self-contained, local, searchable HTML report

**Usage**

```text
Usage: html [OPTIONS]
```

| Argument | Value | Default | Constraints | Description |
| --- | --- | --- | --- | --- |
| `--report` | `REPORT` | `-` | - | Report path. Defaults to .slop/latest/report.json |
| `--output` | `OUTPUT` | `-` | - | Destination. Defaults to .slop/latest/report.html |
| `--include-local-paths` | `flag` | `-` | - | Embed the local source report path in the otherwise portable HTML file |

**Example**

```sh
git slop html --output .slop/latest/report.html
```
