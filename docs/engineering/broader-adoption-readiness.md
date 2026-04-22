# `git-slop` Broader Adoption Readiness

Date: 2026-04-22

## Validation set

### `deeptravel`

- Used as the primary mature-repo validation target.
- The detector produced actionable hotspot queues across maintainer tooling, docs,
  tests, and engine modules.
- The first hotspot tranche and the follow-on maintenance lane both produced
  reviewable refactor work without requiring cross-repo scoring changes.

### `coreycoto.com`

- Needed repo-local scope tuning only.
- The checked-in `.slop/config.yaml` ignores:
  - `data/name-pairs/**`
  - `apps/about/public/experience/*.svg`
- After that tuning, the queue remained dominated by plausible maintained
  code/docs/workflow surfaces such as:
  - `.github/workflows/README.md`
  - `AGENTS.md`
  - `docs/ci-cd.md`
  - `tests/e2e/index.ts`
  - `tools/ci/comment-playwright-regression.ts`

### `neuroscribes`

- Needed repo-local scope tuning only.
- The checked-in `.slop/config.yaml` ignores:
  - `test/data/*.pdf.txt`
  - `output/**/thread_messages.md`
- After that tuning, the queue remained dominated by plausible maintained
  code/config/docs surfaces such as:
  - `src/components/OpenAIServiceProvider.ts`
  - `tsconfig.json`
  - `src/index.ts`
  - `src/components/VercelCacheProvider.ts`
  - `src/components/FileProcessor.ts`

## Decision rule

- If all three repos are credible with only repo-local scope tuning and no
  cross-repo scoring change, `git-slop` is ready for broader adoption.
- Otherwise, open one new `git-slop` core follow-up and defer broader rollout
  until it is resolved.

## Decision

`git-slop` is ready for broader adoption.

Rationale:

- `deeptravel` validated the detector on a mature, high-history repo and turned
  the output into successful queued refactor work.
- `coreycoto.com` and `neuroscribes` each required only narrow repo-local ignore
  rules for known data/generated artifacts.
- No cross-repo scoring or heuristic change was needed to make the queues
  credible.
- Lockfile exclusion and current detector defaults remain appropriate.

## Follow-up boundary

Future repo onboarding should start with defaults first, then add repo-local
`.slop/config.yaml` only when the queue noise is clearly tied to local
data/generated assets. Global scoring changes should remain blocked on evidence
from multiple repos, not on single-repo tuning needs.
