# .slop Directory Policy

Git Slop writes repository-local state under `.slop/`.

```text
.slop/
  config.yaml
  .gitignore
  latest/
    report.json
    report.yaml  # only when output.yaml is true
    summary.md
    health.md
  runs/
    <timestamp>/
      report.json
      report.yaml  # only when output.yaml is true
      summary.md
      health.md
  cache/
  scan.lock
  scan.lock.owner  # present only while a scan owns the lock
  prompt-packs/
  diagnostic-bundle.json
```

## Commit

Commit these files when the repository intentionally adopts Git Slop:

- `.slop/config.yaml`
- `.slop/.gitignore`

`config.yaml` is the repo-owned detector configuration. `.gitignore` keeps
routine generated state out of source control.

## Do Not Commit Routine Outputs

Do not commit these runtime outputs:

- `.slop/latest/`
- `.slop/runs/`
- `.slop/cache/`
- `.slop/prompt-packs/`
- `.slop/scan.lock`
- `.slop/scan.lock.owner`
- `.slop/diagnostic-bundle.json`
- `.slop/config.yaml.bak` and `.slop/.gitignore.bak`
- generated SARIF files
- generated plan JSON
- generated compare JSON

Use CI artifacts or local scratch paths for those files. They are derived from
the repo state and should be regenerated when needed.

The GitHub Action follows a bounded upload policy instead of uploading either
runtime directory:

- default `summary`: `health.md` only
- opt-in `report`: `health.md` and `report.json`
- opt-in `full`: the three default files plus `report.yaml` when YAML is enabled

The default artifact retention is 14 days. Prefer the default unless a machine
consumer needs schema-5 JSON or a reviewer explicitly needs the full bundle.

## Exceptions

Only check in generated-looking artifacts when they are deliberately curated as
examples or fixtures outside the runtime `.slop/` tree. Common examples:

- `tests/fixtures/...`
- documentation snippets with small hand-edited samples
- versioned files attached to GitHub Releases, not committed to Git

If a consumer repo needs stable evidence for review, prefer an uploaded CI
artifact or a link to a GitHub Release asset over committing `.slop/latest/`.

## Cache Notes

`.slop/cache/` is the default packed token-cache location. It is generated
state reserved for deterministic performance optimizations, is safe to delete,
and must never be required for correctness. `git slop cache` defaults to this
same mutable state root; `--state-dir` selects another root explicitly.

## Bundle Notes

`find` writes compact `report.json`, `summary.md`, and `health.md` to both
destinations. YAML is an explicit compatibility export (`output.yaml: true`). The latest
bundle is replaced atomically so consumers do not observe a partially updated
report set. Timestamped run directories are immutable snapshots of individual
detector runs. A process-level `scan.lock` and transient `scan.lock.owner` under
the selected state root prevent concurrent publication. The detector excludes
both coordination files from its own worktree snapshot. Named baselines are the
separate exception: they default to Git-private `git-slop/baselines` storage.

`git slop find --ephemeral` places both scan state and reports under
Git-private `git-slop/ephemeral` storage and disables the token cache. It is a
first-look workflow, not durable repository adoption.

`health`, `show`, `explain`, `plan`, `check`, and `sarif` read an existing
report. `compare` reads two. They do not create another detector run; only
explicit prompt-pack, SARIF output, or redirected command output writes
additional generated files.
