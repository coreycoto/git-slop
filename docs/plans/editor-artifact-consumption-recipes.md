# Editor Artifact Consumption Recipes

Issue: #31

## Purpose

These recipes show how to consume Git Slop's existing report artifacts from
editor-adjacent workflows without adding an editor extension, language server,
background watcher, hosted service, or model runtime.

The supported contracts are:

- SARIF 2.1.0 from `git-slop sarif`
- schema-v2 `plan` JSON from `git-slop plan --format json`
- schema-v1 `refactor-preview` JSON from `git-slop refactor-preview --format json`

All outputs are context-cost and maintenance signals. They are not correctness
proofs, bug reports, or refactor mandates.

## Generate Local Artifacts

Start from a normal local report run:

```bash
git-slop find
```

Export SARIF diagnostics from the latest schema-3 report:

```bash
git-slop sarif \
  --report .slop/latest/report.json \
  --output .slop/latest/git-slop.sarif
```

Generate a bounded maintenance plan for a selected path:

```bash
git-slop plan \
  --path src/git_slop \
  --format json > .slop/latest/plan.json
```

Generate preview-only maintainer steps from that saved plan:

```bash
git-slop refactor-preview \
  --plan .slop/latest/plan.json \
  --format json > .slop/latest/refactor-preview.json
```

Use a relationship selector when the reviewed report evidence points at a
specific relationship:

```bash
git-slop plan \
  --relationship near_duplicate_neighborhood-1234 \
  --format json > .slop/latest/plan.json
```

## SARIF Diagnostics Recipe

Use SARIF when an editor or adjacent tool can render static analysis diagnostics
from a file. The SARIF export contains action-queue findings and preserves Git
Slop hotspot costs and overlay evidence as separate properties.

Recommended workflow:

1. Run `git-slop find`.
2. Run `git-slop sarif --report .slop/latest/report.json --output .slop/latest/git-slop.sarif`.
3. Point the editor or SARIF viewer at `.slop/latest/git-slop.sarif`.
4. Treat rendered findings as maintenance triage hints, not test failures.

Do not upload SARIF automatically from this recipe. Uploading to code scanning
or another hosted service is a separate repository governance decision.

## Plan JSON Recipe

Use plan JSON when the editor-adjacent workflow needs bounded scope and evidence
instead of line-oriented diagnostics. Plan JSON includes stable slice IDs, scope
paths, out-of-scope paths, supporting evidence, evidence summaries, and
preview-only backlog handoff metadata.

Recommended workflow:

1. Pick a reviewed selector from the report, such as a path, cluster, or
   relationship.
2. Run `git-slop plan --format json` for that selector.
3. Render each proposed slice as read-only maintainer context.
4. Preserve the slice boundaries when creating follow-up tasks or backlog
   previews.

Editors and task runners should not infer missing detector intent from this
payload. If evidence is absent or weak, the UI should say so instead of filling
in a stronger claim.

## Refactor Preview JSON Recipe

Use refactor-preview JSON when a maintainer wants concrete next steps before
editing code. The payload carries the source plan selector, selected slice IDs,
scope, out-of-scope paths, evidence, proposed maintainer steps, review checklist
items, non-mutating patch-preview notes, and `mutation_policy: "preview_only"`.

Recommended workflow:

1. Save a plan payload to `.slop/latest/plan.json`.
2. Run `git-slop refactor-preview --plan .slop/latest/plan.json --format json`.
3. Render the preview as a task checklist or side panel.
4. Require a human maintainer to decide whether to edit code.

This payload does not generate diffs, edit files, invoke a model, rerun the
detector, commit, push, or mutate GitHub.

## VS Code Task-Style Recipe

The following is a copy-paste task example for local experimentation. It is not
checked into the repo and is not required by Git Slop.

```json
{
  "version": "2.0.0",
  "tasks": [
    {
      "label": "git-slop: refresh artifacts",
      "type": "shell",
      "command": "git-slop find && git-slop sarif --report .slop/latest/report.json --output .slop/latest/git-slop.sarif",
      "problemMatcher": []
    },
    {
      "label": "git-slop: plan current package",
      "type": "shell",
      "command": "git-slop plan --path src/git_slop --format json > .slop/latest/plan.json && git-slop refactor-preview --plan .slop/latest/plan.json --format json > .slop/latest/refactor-preview.json",
      "problemMatcher": []
    }
  ]
}
```

Keep task commands explicit and manually triggered. Do not add a background
watcher that reruns the detector on file changes.

## Safety Boundaries

Editor-adjacent consumers must preserve Git Slop's product boundaries:

- no first-party editor extension in this slice
- no language server
- no background detector or file watcher
- no automatic code mutation
- no automatic commits or pushes
- no hosted API calls
- no model scoring
- no overlay score inflation
- no changes to `priority_score`, `priority_band`, `context_band`, or
  `git slop check`

If an adapter or editor surface summarizes these artifacts, it should describe
them as local maintenance context and keep detector truth separate from any
human or model-authored interpretation.
