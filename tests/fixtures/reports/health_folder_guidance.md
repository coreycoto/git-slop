# Repository Health

⚠️ **Advisory** — no actionable file breach was found; 1 derived/classified file(s) and 2 folder(s) exceed context budgets as investigation context.

- **Generated at:** `2026-08-06T12:00:00Z`
- **Repo:** `health-folder-guidance`
- **Branch:** `main`
- **Head SHA:** `none`

## Summary

### `agent_context`

> **Reading the signals:** Context/load bands measure deterministic size against configured context budgets. Maintenance pressure and review severity are separate deterministic review signals; neither is a correctness claim or an automatic refactor mandate.

#### Context/load status

| Context/load band | Definition | Files |
| --- | --- | ---: |
| `compact` | `<= 3,072` tokens | 10 |
| `healthy` | `3,073-8,000` tokens | 0 |
| `warning` | `8,001-10,000` tokens | 0 |
| `budget_exceeded` | `>10,000` tokens | 1 |

| Direct-load band | Definition | Folders |
| --- | --- | ---: |
| `compact` | direct tokens `<= 500` | 0 |
| `healthy` | direct tokens `501-2,000` | 0 |
| `warning` | direct tokens `2,001-4,000` or direct files `>2` | 1 |
| `budget_exceeded` | direct tokens `>4,000` or direct files `>4` | 2 |

#### Token Stats

| Type | p50 | p90 | p95 | p99 | Max | Top 1 share | Top 5 share | Top 10 share |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| `files` | 900 | 2,500 | 51,249.50 | 90,249.10 | 99,999 | 93.1% | 97.9% | 99.9% |
| `folders` | 4,500 | 81,139.20 | 90,719.10 | 98,383.02 | 100,299 | 93.5% | 100.0% | 100.0% |

## Investigation Candidates

### Context Budget Exceeded

#### File Risks

| Path | Class | Tokens | Context/load band | Maintenance pressure | % of parent |
| --- | --- | ---: | --- | --- | ---: |
| `src/files-only/generated.json` | data | 99,999 | `budget_exceeded` | `high` · score 99.0 | 99.6% |

#### Folder Risks

| Path | Class | Direct load | Direct-load band and trigger | Maintenance pressure | Highest-ranked descendant | Next step |
| --- | --- | --- | --- | --- | --- | --- |
| `src/files-only` | source | 4 files · 100,299 tokens · 93.3% of parent | `budget_exceeded` — tokens: 100,299 direct tokens \> 4,000 warning ceiling | `high` · score 100.0 | `src/files-only/nested/winner.rs` — maintenance `moderate` · score 50.0; context/load `compact` · 150 tokens | `git slop explain --path src/files-only/` |
| `src/both` | source | 5 files · 4,500 tokens · 4.2% of parent | `budget_exceeded` — both: 5 direct files \> 4 warning ceiling; 4,500 direct tokens \> 4,000 warning ceiling | `high` · score 90.0 | `src/both/0.rs` — maintenance `high` · score 80.0; context/load `compact` · 900 tokens | `git slop explain --path src/both/` |


### Review Candidates

#### Folder Risks

| Path | Class | Direct load | Direct-load band and trigger | Maintenance pressure | Highest-ranked descendant | Next step |
| --- | --- | --- | --- | --- | --- | --- |
| `src/tokens-only` | source | 1 files · 2,500 tokens · 2.3% of parent | `warning` — tokens: 2,500 direct tokens \> 2,000 healthy ceiling | `moderate` · score 61.2 | `src/tokens-only/a.rs` — maintenance `moderate` · score 60.0; context/load `compact` · 2,500 tokens | `git slop explain --path src/tokens-only/` |


## Advisory Health Findings

Showing 5 of 5 advisory finding(s), ordered by review severity and then maintenance pressure. Use `git slop list health-findings --top 5` to inspect the bounded collection.

| Review severity | Path | Context/load band | Maintenance pressure | Why it surfaced | Next step |
| --- | --- | --- | --- | --- | --- |
| `warning` | `src/both/0.rs` | `compact` | `high` · score 80.0 | 900 tokens leave limited context headroom | `git slop explain --path src/both/0.rs` |
| `warning` | `src/both/1.rs` | `compact` | `high` · score 79.0 | 900 tokens leave limited context headroom | `git slop explain --path src/both/1.rs` |
| `warning` | `src/both/2.rs` | `compact` | `high` · score 78.0 | 900 tokens leave limited context headroom | `git slop explain --path src/both/2.rs` |
| `warning` | `src/both/3.rs` | `compact` | `high` · score 77.0 | 900 tokens leave limited context headroom | `git slop explain --path src/both/3.rs` |
| `warning` | `src/both/4.rs` | `compact` | `high` · score 76.0 | 900 tokens leave limited context headroom | `git slop explain --path src/both/4.rs` |

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
