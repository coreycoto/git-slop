# Editor Integration Research

Issue: #24

## Recommendation

Defer a first-party editor extension for now. The first editor-facing surface
should be documented consumption of existing `git-slop` outputs:

1. SARIF for diagnostics in editor ecosystems that already understand SARIF.
2. `git slop plan --format json` for bounded maintenance context.
3. `git slop refactor-preview --plan <plan.json> --format json` for explicit
   preview-only next steps.

This keeps V3 local-first and report-based. It avoids introducing a long-lived
editor process, file watchers, language server state, or background detector
runs before the report contracts have more adoption evidence.

## Evaluation

### SARIF / Code Scanning

SARIF is the best first editor-facing bridge because it is already a standard
diagnostic interchange format and Git Slop now exports action-queue findings as
SARIF 2.1.0. It maps naturally to file locations and severity without changing
detector truth.

Use SARIF when the editor or surrounding toolchain can render static analysis
diagnostics from a file. A maintainer can run:

```bash
git-slop find
git-slop sarif --report .slop/latest/report.json --output .slop/latest/git-slop.sarif
```

The editor-facing guidance should make clear that SARIF findings are
maintainability signals, not correctness failures.

### JSON Plan And Refactor Preview Payloads

Plan and refactor-preview JSON are the right surfaces for richer editor panels
or task runners. They already carry stable slice IDs, scoped paths,
out-of-scope paths, evidence summaries, and preview-only boundaries.

Use these payloads when the editor workflow needs more than diagnostics:

```bash
git-slop plan --path src/git_slop --format json > .slop/latest/plan.json
git-slop refactor-preview --plan .slop/latest/plan.json --format json
```

An editor adapter should render these payloads as read-only context and should
not infer missing detector intent.

### Language Server

A language-server-style integration is not justified yet. It would add a
long-running process and richer editor lifecycle concerns before there is
evidence that maintainers need live diagnostics rather than explicit report
generation.

A language server should only be reconsidered after SARIF and JSON consumption
show that users need incremental refresh, navigation commands, or cross-file
panels that cannot be served by static artifacts.

## Required Contracts

Any editor-facing adapter should consume only existing public artifacts:

- schema-3 `report.json`
- SARIF 2.1.0 from `git slop sarif`
- schema-v2 `plan` JSON
- schema-v1 `refactor-preview` JSON
- optional `compare` JSON for trend views

The adapter should not call internal detector APIs. It should invoke the CLI or
read explicit artifacts under `.slop/`.

## Safety Boundaries

Editor-facing integrations must preserve the current product boundaries:

- no background code mutation
- no automatic refactors
- no automatic commits or pushes
- no hosted API calls
- no LLM scoring
- no overlay score inflation
- no changes to `priority_score`, `priority_band`, `context_band`, or
  `git slop check`
- no hidden detector reruns from an editor extension

Editor UI copy should describe findings as context-cost and maintenance signals.
It must not present them as proof of bugs, correctness failures, or mandatory
refactors.

## Proposed Child Issues

If the project proceeds after this research, split implementation into small
child issues:

1. Document editor consumption recipes for SARIF and JSON artifacts.
2. Add fixture-backed examples for SARIF, plan JSON, and refactor-preview JSON.
3. Prototype a no-runtime editor task recipe for VS Code using existing CLI
   commands.
4. Reassess whether a language server is warranted after recipe adoption.

Do not start a first-party extension or language server until the documentation
and examples show a real gap.

## Completion Decision

Issue #24 can be closed with this decision: Git Slop should not implement a
first-party editor extension yet. The next step is documentation for static
artifact consumption, starting with SARIF and JSON recipe examples.
