# Git Slop CLI Reference: `list`

Generated from the live Clap command tree.

## `git-slop list`

List findings, relationships, clusters, or profiles

**Usage**

```text
Usage: git-slop list <COMMAND>
```

**Machine contract:** `list-1`.

**Example**

```sh
git slop list findings --top 20
```

## `git-slop list findings`

**Usage**

```text
Usage: git-slop list findings [OPTIONS]
```

**Machine contract:** `list-1`.

| Argument | Value | Default | Constraints | Description |
| --- | --- | --- | --- | --- |
| `--report` | `REPORT` | `-` | - | Report path. Defaults to .slop/latest/report.json |
| `--require-current` | `flag` | `-` | - | Fail when the report does not match current HEAD, worktree, config, scope, or analyzer |
| `--top` | `TOP` | `50` | - | Maximum number of matched records to return |
| `--format` | `FORMAT` | `text` | values: text, json, yaml | Output format |
| `--wide` | `flag` | `-` | - | Use a wider terminal layout before truncating fields |
| `--no-truncate` | `flag` | `-` | - | Never truncate terminal fields |
| `--path` | `PATH` | `-` | - | Match a finding path, relationship endpoint, or cluster member |
| `--profile` | `PROFILE` | `-` | - | Match an analysis profile |
| `--language` | `LANGUAGE` | `-` | - | Match a resolved file language |
| `--classification` | `CLASSIFICATION` | `-` | - | Match a resolved file classification |
| `--severity` | `SEVERITY` | `-` | - | Match a finding severity |

**Example**

```sh
git slop list findings --top 20
```

## `git-slop list relationships`

**Usage**

```text
Usage: git-slop list relationships [OPTIONS]
```

**Machine contract:** `list-1`.

| Argument | Value | Default | Constraints | Description |
| --- | --- | --- | --- | --- |
| `--report` | `REPORT` | `-` | - | Report path. Defaults to .slop/latest/report.json |
| `--require-current` | `flag` | `-` | - | Fail when the report does not match current HEAD, worktree, config, scope, or analyzer |
| `--top` | `TOP` | `50` | - | Maximum number of matched records to return |
| `--format` | `FORMAT` | `text` | values: text, json, yaml | Output format |
| `--wide` | `flag` | `-` | - | Use a wider terminal layout before truncating fields |
| `--no-truncate` | `flag` | `-` | - | Never truncate terminal fields |
| `--path` | `PATH` | `-` | - | Match a relationship endpoint |
| `--profile` | `PROFILE` | `-` | - | Match an endpoint analysis profile |
| `--language` | `LANGUAGE` | `-` | - | Match an endpoint file language |
| `--classification` | `CLASSIFICATION` | `-` | - | Match an endpoint file classification |

**Example**

```sh
git slop list relationships --top 20
```

## `git-slop list clusters`

**Usage**

```text
Usage: git-slop list clusters [OPTIONS]
```

**Machine contract:** `list-1`.

| Argument | Value | Default | Constraints | Description |
| --- | --- | --- | --- | --- |
| `--report` | `REPORT` | `-` | - | Report path. Defaults to .slop/latest/report.json |
| `--require-current` | `flag` | `-` | - | Fail when the report does not match current HEAD, worktree, config, scope, or analyzer |
| `--top` | `TOP` | `50` | - | Maximum number of matched records to return |
| `--format` | `FORMAT` | `text` | values: text, json, yaml | Output format |
| `--wide` | `flag` | `-` | - | Use a wider terminal layout before truncating fields |
| `--no-truncate` | `flag` | `-` | - | Never truncate terminal fields |
| `--path` | `PATH` | `-` | - | Match a cluster member path |
| `--profile` | `PROFILE` | `-` | - | Match a member analysis profile |
| `--language` | `LANGUAGE` | `-` | - | Match a member file language |
| `--classification` | `CLASSIFICATION` | `-` | - | Match a member file classification |

**Example**

```sh
git slop list clusters --top 20
```

## `git-slop list profiles`

**Usage**

```text
Usage: git-slop list profiles [OPTIONS]
```

**Machine contract:** `list-1`.

| Argument | Value | Default | Constraints | Description |
| --- | --- | --- | --- | --- |
| `--report` | `REPORT` | `-` | - | Report path. Defaults to .slop/latest/report.json |
| `--require-current` | `flag` | `-` | - | Fail when the report does not match current HEAD, worktree, config, scope, or analyzer |
| `--top` | `TOP` | `50` | - | Maximum number of matched records to return |
| `--format` | `FORMAT` | `text` | values: text, json, yaml | Output format |
| `--wide` | `flag` | `-` | - | Use a wider terminal layout before truncating fields |
| `--no-truncate` | `flag` | `-` | - | Never truncate terminal fields |
| `--profile` | `PROFILE` | `-` | - | Match an analysis profile |

**Example**

```sh
git slop list profiles --format json
```
