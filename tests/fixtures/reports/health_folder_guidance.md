# Repository Health

❌ **Review required** — 0 file(s) and 1 folder(s) exceed configured refactor thresholds.

- **Generated at:** `2026-08-06T12:00:00Z`
- **Repo:** `health-folder-guidance`
- **Branch:** `main`
- **Head SHA:** `abc123`

## Summary

### `agent_context`

> **Reading the signals:** Context/load bands measure deterministic size against configured context budgets. Maintenance pressure and review severity are separate deterministic review signals; neither is a correctness claim or an automatic refactor mandate.

#### Context/load status

| Context/load band | Definition | Files |
| --- | --- | ---: |
| `compact` | `<= 3,072` tokens | 10 |
| `healthy` | `3,073-8,000` tokens | 0 |
| `warning` | `8,001-10,000` tokens | 0 |
| `refactor_required` | `>10,000` tokens | 0 |

| Direct-load band | Definition | Folders |
| --- | --- | ---: |
| `compact` | direct tokens `<= 500` | 0 |
| `healthy` | direct tokens `501-2,000` | 0 |
| `warning` | direct tokens `2,001-4,000` or direct files `>2` | 2 |
| `refactor_required` | direct tokens `>4,000` or direct files `>4` | 1 |

#### Token Stats

| Type | p50 | p90 | p95 | p99 | Max | Top 1 share | Top 5 share | Top 10 share |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| `files` | 900 | 1,060 | 1,780 | 2,356 | 2,500 | 33.6% | 81.9% | 100.0% |
| `folders` | 2,500 | 4,100 | 4,300 | 4,460 | 4,500 | 61.6% | 100.0% | 100.0% |

## Refactor Recommendations

### Policy Failures

#### Folder Risks

| Path | Class | Direct load | Direct-load band and trigger | Maintenance pressure | Highest-ranked descendant | Next step |
| --- | --- | --- | --- | --- | --- | --- |
| `src/both` | source | 5 files · 4,500 tokens · 60.4% of parent | `refactor_required` — both: 5 direct files \> 4 warning ceiling; 4,500 direct tokens \> 4,000 warning ceiling | `critical` · score 90.0 | `src/both/0.rs` — maintenance `high` · score 80.0; context/load `compact` · 900 tokens | `git-slop explain --path src/both/` |


### Review Candidates

#### Folder Risks

| Path | Class | Direct load | Direct-load band and trigger | Maintenance pressure | Highest-ranked descendant | Next step |
| --- | --- | --- | --- | --- | --- | --- |
| `src/tokens-only` | source | 1 files · 2,500 tokens · 33.6% of parent | `warning` — tokens: 2,500 direct tokens \> 2,000 healthy ceiling | `moderate` · score 61.2 | `src/tokens-only/a.rs` — maintenance `moderate` · score 60.0; context/load `compact` · 2,500 tokens | `git-slop explain --path src/tokens-only/` |
| `src/files-only` | source | 3 files · 300 tokens · 4.0% of parent | `warning` — files: 3 direct files \> 2 healthy ceiling | `high` · score 1,234.5 | `src/files-only/nested/winner.rs` — maintenance `moderate` · score 50.0; context/load `compact` · 150 tokens | `git-slop explain --path src/files-only/` |


## Actionable Findings

| Review severity | Path | Context/load band | Maintenance pressure | Why it surfaced | Next step |
| --- | --- | --- | --- | --- | --- |
| `warning` | `src/both/0.rs` | `compact` | `high` · score 80.0 | 900 tokens leave limited context headroom | `git-slop explain --path src/both/0.rs` |
| `warning` | `src/both/1.rs` | `compact` | `high` · score 79.0 | 900 tokens leave limited context headroom | `git-slop explain --path src/both/1.rs` |
| `warning` | `src/both/2.rs` | `compact` | `high` · score 78.0 | 900 tokens leave limited context headroom | `git-slop explain --path src/both/2.rs` |
| `warning` | `src/both/3.rs` | `compact` | `high` · score 77.0 | 900 tokens leave limited context headroom | `git-slop explain --path src/both/3.rs` |
| `warning` | `src/both/4.rs` | `compact` | `high` · score 76.0 | 900 tokens leave limited context headroom | `git-slop explain --path src/both/4.rs` |

## Rollups

<details>
<summary>By profile and language</summary>

### By Profile

| Profile | Files | Lines | Code | Comments | Blanks | Tokens |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| `agent_context` | 10 | 0 | 0 | 0 | 0 | 7,450 |
| `data_context` | 1 | 0 | 0 | 0 | 0 | 99,999 |

### By Language · `agent_context`

| Language | Files | Lines | Tokens | % of profile tokens |
| --- | ---: | ---: | ---: | ---: |
| Rust | 10 | 0 | 7,450 | 100.0% |

### By Language · `data_context`

| Language | Files | Lines | Tokens | % of profile tokens |
| --- | ---: | ---: | ---: | ---: |
| JSON | 1 | 0 | 99,999 | 100.0% |

</details>

> Git Slop reports deterministic context and maintenance-pressure evidence. Findings are not correctness proofs or automatic refactor mandates.
