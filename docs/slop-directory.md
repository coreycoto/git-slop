# .slop Directory Policy

Git Slop writes repository-local state under `.slop/`.

```text
.slop/
  config.yaml
  .gitignore
  latest/
  runs/
  cache/
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
- generated SARIF files
- generated plan JSON
- generated compare JSON
- generated refactor-preview JSON

Use CI artifacts or local scratch paths for those files. They are derived from
the repo state and should be regenerated when needed.

## Exceptions

Only check in generated-looking artifacts when they are deliberately curated as
examples or fixtures outside the runtime `.slop/` tree. Common examples:

- `tests/fixtures/...`
- documentation snippets with small hand-edited samples
- release assets attached to GitHub Releases, not committed to Git

If a consumer repo needs stable evidence for review, prefer an uploaded CI
artifact or a link to a GitHub Release asset over committing `.slop/latest/`.

## Cache Notes

`.slop/cache/` is an optimization only. It is safe to delete. Cold and warm runs
on the same repo snapshot and config should produce the same report content.
