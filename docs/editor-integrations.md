# Editor-Adjacent Workflows

Git Slop's existing static artifacts cover the validated editor use cases. A
dedicated editor extension or background detector is not currently justified:
it would duplicate report rendering, introduce another update channel, and
risk rescoring stale state. Revisit that decision only with a concrete workflow
that cannot consume the surfaces below.

## Fast local task

Use a disposable Git-private scan when evaluating a repository before adoption:

```bash
git slop find --ephemeral
```

For an adopted repository, a VS Code task can run the normal detector without
embedding output parsing in the editor:

```json
{
  "version": "2.0.0",
  "tasks": [
    {
      "label": "Git Slop: refresh report",
      "type": "shell",
      "command": "git slop find",
      "problemMatcher": []
    }
  ]
}
```

The terminal receipt reports elapsed time, cache hits and misses, peak memory,
report size, and profile. `git slop doctor --require-current` is the local gate
for tasks that must reject stale output.

## Searchable local report

```bash
git slop html --output .slop/latest/report.html
```

The self-contained HTML file provides path search, classification and band
filters, sortable tables, relationships, clusters, and deep-link query state.
It has no network dependency and embeds bounded evidence from one validated
report.

## Diagnostics and SARIF

```bash
git slop sarif --output .slop/latest/git-slop.sarif
git slop doctor --bundle
```

SARIF is the stable interchange for editor or code-scanning viewers. The
doctor bundle is redacted and suitable for issue intake; it excludes source,
raw tokens, credentials, absolute paths, and author identities.

## Delta review

Use native baseline comparison instead of an editor-owned diff model:

```bash
git slop baseline ensure --name before
# make the reviewed change
git slop find
git slop compare --baseline before --detail full --format json \
  > .slop/latest/comparison.json
```

The JSON comparison is deterministic and can be viewed by any editor JSON
viewer. It preserves report identity, compatibility, movement, and regression
semantics without a background service.

## Decision boundary

The validated use cases are refresh, currentness, search, drill-down, SARIF,
diagnostics, and native delta review. None requires an extension today. A future
editor feature should start with a reproducible gap, identify the missing
artifact or navigation contract, and remain a consumer of detector truth.
