# Published JSON Schemas

These files are release artifacts for machine consumers. `git slop config
schema` and `git slop report schema` print the authoritative schemas compiled
into the matching binary. The checked-in copies provide stable URLs and are
validated during release preparation.

- `config-2.json`: `.slop/config.yaml` after YAML-to-JSON conversion
- `report-4.json`: canonical `report.json`
- `compare-1.json`: native `git slop compare --format json`
