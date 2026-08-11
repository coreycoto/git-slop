# Published JSON Schemas

These files are release artifacts for machine consumers. `git slop config
schema` and `git slop report schema` print the authoritative schemas compiled
into the matching binary. The checked-in copies provide stable URLs and are
validated during release preparation.

Schema `$id` values identify the release that introduced each contract, not the
currently running binary. Contract filenames and `schema_version` are the stable
identities; compatible later releases intentionally retain the introduction URL.

- `config-2.json`: `.slop/config.yaml` after YAML-to-JSON conversion
- `report-5.json`: canonical normalized `report.json`
- `report-4.json`: legacy migration input retained for cross-version readers
- `compare-1.json`: native `git slop compare --format json`
- `baseline-1.json`: named baseline create/ensure/list/inspect/update/remove output
- `explain-2.json`, `plan-2.json`, `sarif-1.json`, `health-1.json`,
  `check-1.json`, `doctor-1.json`, `build-info-1.json`, `list-1.json`,
  `show-1.json`, and `prompt-manifest-1.json`: other versioned machine surfaces
  exposed through `git slop schema <contract>`
