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
  `check-1.json`, `doctor-1.json`, `build-info-2.json`, `list-1.json`,
  `show-1.json`, and `prompt-manifest-1.json`: other versioned machine surfaces
  exposed through `git slop schema <contract>`

- `release-manifest-3.json`: public release inventory, provenance, and install
  command contract, exposed with `git slop schema release-manifest`
- `policy-pack-1.json` and `policy-lock-1.json`: data-only policy source and
  deterministic selected-pack resolution contracts
- `advice-input-1.json`, `advice-response-1.json`, and `advice-1.json`:
  provider-independent context, strict provider response, and validated
  non-mutating advice artifact contracts
- `advisor-corpus-1.json`, `advisor-ratings-1.json`, `advisor-ratings-2.json`,
  `advisor-review-artifact-1.json`, `advisor-review-manifest-1.json`,
  `advisor-operation-receipt-1.json`, `advisor-thresholds-1.json`,
  `advisor-benchmark-1.json`, and `advisor-capacity-1.json`: reviewed benchmark
  inputs, stable maintainer-operation receipts, blinded independent
  multi-reviewer evidence, preregistered release thresholds, privacy-safe
  machine-readable performance/quality results, and provider-free host
  eligibility receipts. Ratings schema 1 remains available for shipped
  contract compatibility; finalization requires schema 2.

`advisor-benchmark-1.json` is strict at every nested object and distinguishes
prepare-only, completed, interrupted, unfinalized, finalized, and shippable
states. Maintainer tooling validates the exact schema before each output write;
finalization binds the exact sample matrix and repository evidence to the
pinned corpus, rederives automatic evidence, and binds the immutable source,
private review manifest, and ratings digests.

`build-info-1.json` remains published for consumers of releases through 0.11.x.
