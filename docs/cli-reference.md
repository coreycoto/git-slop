# Git Slop CLI Reference

Generated from the live Clap command tree.

## Exit codes

- `0`: command completed successfully; policy gates passed or were evaluation-only.
- `1`: a valid policy, regression, or adoption check found an unmet condition.
- `2`: command usage, an input contract, or required-currentness validation failed.
- `3`: repository access, report I/O, or another operational dependency failed.
- `4`: a configured or measured resource limit prevented safe completion.

## `git-slop`

Find the files that cost too much context.

**Usage**

```text
Usage: git-slop [OPTIONS] <COMMAND>
```

| Argument | Value | Default | Constraints | Description |
| --- | --- | --- | --- | --- |
| `--repo` | `PATH` | `-` | global | Repository or path inside a repository to analyze |
| `--error-format` | `ERROR_FORMAT` | `human` | global; values: human, json | Render runtime errors as human text or stable JSON |

**Example**

```sh
git slop --help
```

## Commands

- [git-slop init](cli-reference/init.md)
- [git-slop find](cli-reference/find.md)
- [git-slop show](cli-reference/show.md)
- [git-slop explain](cli-reference/explain.md)
- [git-slop plan](cli-reference/plan.md)
- [git-slop check](cli-reference/check.md)
- [git-slop compare](cli-reference/compare.md)
- [git-slop baseline](cli-reference/baseline.md)
- [git-slop report](cli-reference/report.md)
- [git-slop sarif](cli-reference/sarif.md)
- [git-slop health](cli-reference/health.md)
- [git-slop config](cli-reference/config.md)
- [git-slop doctor](cli-reference/doctor.md)
- [git-slop list](cli-reference/list.md)
- [git-slop prune](cli-reference/prune.md)
- [git-slop cache](cli-reference/cache.md)
- [git-slop completions](cli-reference/completions.md)
- [git-slop man](cli-reference/man.md)
- [git-slop reference](cli-reference/reference.md)
- [git-slop html](cli-reference/html.md)
- [git-slop version](cli-reference/version.md)
- [git-slop build-info](cli-reference/build-info.md)
- [git-slop schema](cli-reference/schema.md)
