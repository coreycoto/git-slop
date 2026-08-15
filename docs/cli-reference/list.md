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
git slop list interventions --top 20
```

## `git-slop list policy-failures`

List policy-enforced failures from repository health

**Usage**

```text
Usage: git-slop list policy-failures [OPTIONS]
```

**Machine contract:** `list-1`.

| Argument | Value | Default | Constraints | Description |
| --- | --- | --- | --- | --- |
| `--report` | `REPORT` | `-` | - | Report path. Defaults to the durable latest report, then the Git-private first-run report |
| `--require-current` | `flag` | `-` | - | Fail when the report does not match current HEAD, worktree, config, scope, or analyzer |
| `--top` | `TOP` | `50` | - | Maximum number of matched records to return |
| `--format` | `FORMAT` | `text` | values: text, json, yaml | Output format |
| `--wide` | `flag` | `-` | - | Use a wider terminal layout before truncating fields |
| `--no-truncate` | `flag` | `-` | - | Never truncate terminal fields |
| `--path` | `PATH` | `-` | - | Match a finding path, relationship endpoint, or cluster member |
| `--profile` | `PROFILE` | `-` | - | Match an analysis profile |
| `--language` | `LANGUAGE` | `-` | - | Match a resolved file language |
| `--classification` | `CLASSIFICATION` | `-` | - | Match a resolved file classification |
| `--severity` | `SEVERITY` | `-` | - | Match delivery severity (error, warning, or notice) |
| `--context-band` | `CONTEXT_BAND` | `-` | - | Match detector context band independently of severity |
| `--slop-band` | `SLOP_BAND` | `-` | - | Match detector maintenance-pressure band independently of severity |

**Example**

```sh
git slop list policy-failures --top 20
```

## `git-slop list interventions`

List bounded maintenance candidates that warrant review

**Usage**

```text
Usage: git-slop list interventions [OPTIONS]
```

**Machine contract:** `list-1`.

| Argument | Value | Default | Constraints | Description |
| --- | --- | --- | --- | --- |
| `--report` | `REPORT` | `-` | - | Report path. Defaults to the durable latest report, then the Git-private first-run report |
| `--require-current` | `flag` | `-` | - | Fail when the report does not match current HEAD, worktree, config, scope, or analyzer |
| `--top` | `TOP` | `50` | - | Maximum number of matched records to return |
| `--format` | `FORMAT` | `text` | values: text, json, yaml | Output format |
| `--wide` | `flag` | `-` | - | Use a wider terminal layout before truncating fields |
| `--no-truncate` | `flag` | `-` | - | Never truncate terminal fields |
| `--path` | `PATH` | `-` | - | Match a finding path, relationship endpoint, or cluster member |
| `--profile` | `PROFILE` | `-` | - | Match an analysis profile |
| `--language` | `LANGUAGE` | `-` | - | Match a resolved file language |
| `--classification` | `CLASSIFICATION` | `-` | - | Match a resolved file classification |
| `--severity` | `SEVERITY` | `-` | - | Match delivery severity (error, warning, or notice) |
| `--context-band` | `CONTEXT_BAND` | `-` | - | Match detector context band independently of severity |
| `--slop-band` | `SLOP_BAND` | `-` | - | Match detector maintenance-pressure band independently of severity |

**Example**

```sh
git slop list interventions --top 20
```

## `git-slop list observations`

List observation-only signals that do not request intervention

**Usage**

```text
Usage: git-slop list observations [OPTIONS]
```

**Machine contract:** `list-1`.

| Argument | Value | Default | Constraints | Description |
| --- | --- | --- | --- | --- |
| `--report` | `REPORT` | `-` | - | Report path. Defaults to the durable latest report, then the Git-private first-run report |
| `--require-current` | `flag` | `-` | - | Fail when the report does not match current HEAD, worktree, config, scope, or analyzer |
| `--top` | `TOP` | `50` | - | Maximum number of matched records to return |
| `--format` | `FORMAT` | `text` | values: text, json, yaml | Output format |
| `--wide` | `flag` | `-` | - | Use a wider terminal layout before truncating fields |
| `--no-truncate` | `flag` | `-` | - | Never truncate terminal fields |
| `--path` | `PATH` | `-` | - | Match a finding path, relationship endpoint, or cluster member |
| `--profile` | `PROFILE` | `-` | - | Match an analysis profile |
| `--language` | `LANGUAGE` | `-` | - | Match a resolved file language |
| `--classification` | `CLASSIFICATION` | `-` | - | Match a resolved file classification |
| `--severity` | `SEVERITY` | `-` | - | Match delivery severity (error, warning, or notice) |
| `--context-band` | `CONTEXT_BAND` | `-` | - | Match detector context band independently of severity |
| `--slop-band` | `SLOP_BAND` | `-` | - | Match detector maintenance-pressure band independently of severity |

**Example**

```sh
git slop list observations --top 20
```

## `git-slop list health-findings`

List advisory repository-health findings

**Usage**

```text
Usage: git-slop list health-findings [OPTIONS]
```

**Machine contract:** `list-1`.

| Argument | Value | Default | Constraints | Description |
| --- | --- | --- | --- | --- |
| `--report` | `REPORT` | `-` | - | Report path. Defaults to the durable latest report, then the Git-private first-run report |
| `--require-current` | `flag` | `-` | - | Fail when the report does not match current HEAD, worktree, config, scope, or analyzer |
| `--top` | `TOP` | `50` | - | Maximum number of matched records to return |
| `--format` | `FORMAT` | `text` | values: text, json, yaml | Output format |
| `--wide` | `flag` | `-` | - | Use a wider terminal layout before truncating fields |
| `--no-truncate` | `flag` | `-` | - | Never truncate terminal fields |
| `--path` | `PATH` | `-` | - | Match a finding path, relationship endpoint, or cluster member |
| `--profile` | `PROFILE` | `-` | - | Match an analysis profile |
| `--language` | `LANGUAGE` | `-` | - | Match a resolved file language |
| `--classification` | `CLASSIFICATION` | `-` | - | Match a resolved file classification |
| `--severity` | `SEVERITY` | `-` | - | Match delivery severity (error, warning, or notice) |
| `--context-band` | `CONTEXT_BAND` | `-` | - | Match detector context band independently of severity |
| `--slop-band` | `SLOP_BAND` | `-` | - | Match detector maintenance-pressure band independently of severity |

**Example**

```sh
git slop list health-findings --top 20
```

## `git-slop list findings`

Deprecated compatibility name for `health-findings`

**Usage**

```text
Usage: git-slop list findings [OPTIONS]
```

**Machine contract:** `list-1`.

| Argument | Value | Default | Constraints | Description |
| --- | --- | --- | --- | --- |
| `--report` | `REPORT` | `-` | - | Report path. Defaults to the durable latest report, then the Git-private first-run report |
| `--require-current` | `flag` | `-` | - | Fail when the report does not match current HEAD, worktree, config, scope, or analyzer |
| `--top` | `TOP` | `50` | - | Maximum number of matched records to return |
| `--format` | `FORMAT` | `text` | values: text, json, yaml | Output format |
| `--wide` | `flag` | `-` | - | Use a wider terminal layout before truncating fields |
| `--no-truncate` | `flag` | `-` | - | Never truncate terminal fields |
| `--path` | `PATH` | `-` | - | Match a finding path, relationship endpoint, or cluster member |
| `--profile` | `PROFILE` | `-` | - | Match an analysis profile |
| `--language` | `LANGUAGE` | `-` | - | Match a resolved file language |
| `--classification` | `CLASSIFICATION` | `-` | - | Match a resolved file classification |
| `--severity` | `SEVERITY` | `-` | - | Match delivery severity (error, warning, or notice) |
| `--context-band` | `CONTEXT_BAND` | `-` | - | Match detector context band independently of severity |
| `--slop-band` | `SLOP_BAND` | `-` | - | Match detector maintenance-pressure band independently of severity |

**Example**

```sh
git slop list findings --top 20
```

## `git-slop list relationships`

List evidence-backed relationships between paths

**Usage**

```text
Usage: git-slop list relationships [OPTIONS]
```

**Machine contract:** `list-1`.

| Argument | Value | Default | Constraints | Description |
| --- | --- | --- | --- | --- |
| `--report` | `REPORT` | `-` | - | Report path. Defaults to the durable latest report, then the Git-private first-run report |
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

List structural or consolidation clusters

**Usage**

```text
Usage: git-slop list clusters [OPTIONS]
```

**Machine contract:** `list-1`.

| Argument | Value | Default | Constraints | Description |
| --- | --- | --- | --- | --- |
| `--report` | `REPORT` | `-` | - | Report path. Defaults to the durable latest report, then the Git-private first-run report |
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

List aggregate analysis-profile totals

**Usage**

```text
Usage: git-slop list profiles [OPTIONS]
```

**Machine contract:** `list-1`.

| Argument | Value | Default | Constraints | Description |
| --- | --- | --- | --- | --- |
| `--report` | `REPORT` | `-` | - | Report path. Defaults to the durable latest report, then the Git-private first-run report |
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
