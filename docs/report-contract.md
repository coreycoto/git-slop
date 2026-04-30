# Report and Config Contract

`report.json` is Git Slop's machine contract. Markdown and terminal output are
human conveniences built from the same report data.

## Detector Report

Current report schema:

- `schema_version: 3`

Canonical top-level sections:

- `summary`
- `repo`
- `config`
- `stats`
- `files`
- `folders`
- `action_queue`
- `costs`
- `overlays`

Stable cost sections:

- `costs.load`
- `costs.volatility`
- `costs.coordination`

Overlay sections:

- `overlays.organization_health`
- `overlays.verification`
- `overlays.navigation`
- `overlays.blast_radius`
- `overlays.stewardship`
- `overlays.semantic_drift`

Compatibility mirrors currently emitted for older consumers:

- `organization_metrics`
- `relationships`
- `clusters`

Consumers should prefer the canonical `costs` and `overlays` sections.

## Stable Detector Fields

The stable detector fields are:

- `priority_score`
- `priority_band`
- `context_band`
- `action_queue`

`git slop check` uses stable detector costs only. It ignores overlays.

## Downstream Payloads

Downstream commands consume existing reports and emit additive payloads:

- `git slop explain`: schema-v2 explain payload
- `git slop plan`: schema-v2 plan payload
- `git slop compare`: schema-v1 compare payload from two schema-3 reports
- `git slop sarif`: SARIF 2.1.0 from one schema-3 report

These surfaces are additive. They do not rerun the detector unless explicitly
documented, rescore detector truth, mutate `.slop/`, or change check semantics.

## Config

`.slop/config.yaml` writes:

- `schema_version: 2`

Current config namespaces:

- `inventory`
- `tokenization`
- `history`
- `scoring`
- `organization`
- `verification`
- `navigation`
- `blast_radius`
- `stewardship`
- `semantic_drift`
- `check`

Important defaults:

- organization-health stays always-on
- no user-facing overlay enable/disable switch
- deterministic candidate limiting is allowed internally for performance
- `history.follow_renames: true` remains opt-in

Git Slop still accepts legacy `schema_version: 1` configs and normalizes them
forward during load.
