# Organization Health Validation

Date: 2026-04-22

## Scope

This note records the current evidence-quality pass for the schema-3 detector
and its always-on overlay layer.

Validated repos:

- `git-slop`
- `deeptravel`
- `neuroscribes`

The goal of this pass was not to inflate the hotspot score. It was to confirm
three things:

- the hotspot queue still means context cost, not structural cost
- the overlay layer produces plausible coordination-cost and operational
  evidence
- no cross-repo signal justifies folding overlays into `priority_score`

## Runtime check

The repo-wide history pass materially improved the remaining hot path.

On `deeptravel` with `history.follow_renames: true`:

- cold full-detector run: about `5.3s`
- warm full-detector run: about `3.4s`

That is a large improvement over the earlier history-dominated path that was
closer to ninety seconds. Warm overlay analysis remains effectively negligible
compared with inventory, tokenization, and the repo-wide history parse.

## Repo findings

### `git-slop`

What it validated:

- the hotspot queue still surfaces context-heavy source and test files first
- the overlay layer finds plausible internal coordination patterns in skill
  wrappers, workflow files, docs, and checked-in assets
- self-dogfood findings remain explainable without touching the main score

Top hotspot examples:

- `src/git_slop/organization.py`
- `tests/test_detector_integration.py`
- `src/git_slop/reporting.py`
- `src/git_slop/history.py`

Top structural examples:

- duplicate or near-duplicate skill wrapper scripts
- shared workflow structure across `.github/workflows/*.yml`
- cross-doc coordination between `README.md`, `docs/architecture.md`,
  `docs/scoring-model.md`, and `docs/vision.md`
- verification and navigation pressure concentrated in high-context maintainer
  surfaces

Accepted repo-local noise:

- repeated skill wrappers are real structural findings inside this repo, but
  they are also partly an intentional outcome of the checked-in skill
  packaging model
- this is useful evidence, not a reason to change global scoring

### `deeptravel`

What it validated:

- the hotspot queue still behaves like context-cost ranking on a mature,
  high-history repo
- the overlay layer finds large coordination surfaces that the hotspot queue
  does not try to compress into one scalar
- rename-aware history remains compatible with the organization-health overlay

Top hotspot examples:

- `tests/unit/deeptravel_maintainers/github/governance/test_backlog_mutations.py`
- `tests/unit/deeptravel_maintainers/github/reviews/test_apply_review_backlog_delta.py`
- `schemas/shared.schema.json`
- `tests/unit/deeptravel_maintainers/testing/test_slow_test_audit_validation.py`
- `tests/unit/deeptravel/cli/test_doctor.py`

Top structural examples:

- repeated coordination across `.agents/skills/*/SKILL.md`
- duplicate and near-duplicate output schemas
- large consolidation candidates across shared eval/reference/config assets
- boundary-heavy clusters across commands, domain, engine, and maintainer
  surfaces
- verification gaps and blast-radius pressure on mature operational code paths

Accepted repo-local noise:

- golden fixture JSON files dominate some duplicate/near-duplicate
  relationships, which is expected for a repo with large checked-in fixture
  corpora
- shared eval/config/schema assets create very large consolidation candidates;
  those are useful structural evidence, but they are not an argument for
  changing the main hotspot queue

### `neuroscribes`

What it validated:

- the defaults plus narrow repo-local config are enough for a smaller
  TypeScript-heavy repo
- the hotspot queue remains readable and still points at maintained code/config
  surfaces
- the overlay layer can catch plausible duplicated concepts without needing
  cross-repo heuristic changes

Top hotspot examples:

- `src/components/OpenAIServiceProvider.ts`
- `tsconfig.json`
- `src/index.ts`
- `src/components/VercelCacheProvider.ts`
- `src/components/FileProcessor.ts`

Top structural examples:

- `src/agents/Emojicrafter.ts` and `src/assistants/emojicrafter.ts`
- a consolidation candidate spanning assistants, agents, and shared component
  interfaces

Accepted repo-local noise:

- `.slop/config.yaml` itself surfaced as a recent structural file because it was
  newly added and touched recently; that is acceptable local noise
- the already-tuned repo-local ignores remain the correct place to suppress
  generated text artifacts, not the detector core

## Cross-repo decision

No detector-level follow-up was opened from this pass.

Reasoning:

- each repo produced plausible coordination-cost evidence
- the noisiest findings were repo-local and understandable
- no single structural pattern appeared across at least two repos in a way that
  justified changing global weights, thresholds, or CLI behavior

## Explicit boundary

`priority_score` remains untouched.

- It still represents context cost only: size, age, and churn.
- Overlay evidence remains parallel:
  - `duplication_pressure`
  - `diffusion_pressure`
  - `coupling_pressure`
  - `boundary_pressure`
  - `verification_gap`
  - `navigation_pressure`
  - `blast_radius_pressure`
  - `stewardship_pressure`
  - `semantic_drift_pressure`
- `git slop check` still ignores overlay output entirely.

The current detector should therefore be treated as:

- one stable queue for context-heavy files
- one experimental evidence layer for structural and operational findings

That separation remains the right product boundary for the next wave.
