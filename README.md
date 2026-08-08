<h1>
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="assets/brand/git-slop-inverse.svg">
    <img src="assets/brand/git-slop.svg" alt="" width="32" height="32">
  </picture>
  Git Slop
</h1>

**A language-agnostic token defragmenter for human-and-agent software
development.**

[![Crates.io](https://img.shields.io/crates/v/git-slop.svg)](https://crates.io/crates/git-slop)
[![CI](https://github.com/coreycoto/git-slop/actions/workflows/ci.yml/badge.svg)](https://github.com/coreycoto/git-slop/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

Git Slop is an open-source, local-first Rust tool that finds where token
distribution and fragmentation make a repository expensive to load, reason
about, and safely change. It turns tracked files and Git history into
deterministic health reports and bounded maintenance plans for people and
coding agents.

> Models matter. Repository shape is part of the inference bill too.

Git Slop does not try to determine whether AI wrote your code. It measures the
repository that exists.

## Quick Start

Install with Homebrew on macOS or Linux:

```bash
brew install coreycoto/tap/git-slop
```

Then run Git Slop inside any Git repository:

```bash
git slop init
git slop find
git slop health
git slop explain --top 5
```

`init` creates a repo-owned configuration and ignore rules. `find` performs the
analysis once; the other commands read the generated report without rescoring
it. Run from a full-history checkout when age, churn, coupling, and stewardship
evidence matter.

The health dashboard points to the next useful command. After reviewing a
finding, ask Git Slop for a bounded maintenance proposal:

```bash
git slop explain --path src/example.rs
git slop plan --path src/example.rs
```

A plan is evidence for human review. It does not edit code, invoke a model, or
mutate Git or GitHub.

## What Git Slop Makes Visible

Repositories become expensive in more than one way. Git Slop keeps those costs
separate so maintainers can see why something surfaced.

| Signal | Question it helps answer |
| --- | --- |
| Context load | Which files and folders consume too much working context? |
| Maintenance pressure | Which expensive surfaces are old, volatile, or repeatedly revised? |
| Fragmentation | Where is one concept duplicated, scattered, or leaking across boundaries? |
| Coordination | Which paths repeatedly change together or carry a broad blast radius? |
| Verification | Which hotspots have weak nearby test or test-co-change evidence? |
| Navigation, stewardship, and drift | Where is knowledge hard to find, ownership concentrated, or terminology diverging? |

The stable hotspot score uses deterministic context, age, and churn evidence.
Coordination and structural overlays remain separate supporting evidence; they
cannot silently inflate `slop_score` or change `git slop check`.

A hotspot is not a correctness verdict or an automatic refactor order. It is a
place where the cost of future work deserves investigation.

## From Evidence to Bounded Work

1. **Measure:** `git slop find` inventories tracked text files and mines local
   Git history.
2. **Orient:** `git slop health` summarizes repository shape and recommends
   deterministic drill-down commands.
3. **Investigate:** `show` and `explain` connect a hotspot to the evidence that
   surfaced it.
4. **Bound:** `plan` proposes small maintenance slices with explicit scope,
   exclusions, and verification evidence.
5. **Decide and verify:** a person chooses the work, a human or coding agent
   implements it, then reruns `find` and uses `check` or `compare` to measure
   the result.

Git Slop remains observational throughout that loop. The repository owner keeps
the judgment.

## Report Bundle

Every successful `find` writes the same four-file bundle to `.slop/latest/` and
a timestamped copy under `.slop/runs/`:

| Artifact | Purpose |
| --- | --- |
| `report.json` | Versioned machine contract for automation |
| `report.yaml` | Equivalent machine data for YAML consumers |
| `summary.md` | Detailed detector and overlay evidence |
| `health.md` | Concise repository-health dashboard for people and CI |

Routine generated output stays untracked. Commit `.slop/config.yaml` and
`.slop/.gitignore` when a repository intentionally adopts Git Slop; see the
[`.slop` directory policy](docs/slop-directory.md).

## Install

The examples below use Git Slop 0.9.6. See [Installation](docs/install.md) for
release archives, upgrades, provenance details, and contributor setup.

### Homebrew (macOS and Linux)

```bash
brew install coreycoto/tap/git-slop
```

### Cargo

```bash
cargo install git-slop --version 0.9.6 --locked
```

### Scoop (Windows)

```powershell
scoop bucket add coreycoto https://github.com/coreycoto/scoop-bucket
scoop install coreycoto/git-slop
```

Verify an installed release and its source provenance:

```bash
git-slop version
git-slop build-info --format json
```

The executable is `git-slop`. When it is on `PATH`, Git also accepts
`git slop`.

## GitHub Actions

Use a full-history checkout for complete history-derived evidence:

```yaml
permissions:
  contents: read

steps:
  - uses: actions/checkout@v7
    with:
      fetch-depth: 0
  - uses: coreycoto/git-slop@v0.9.6
```

The Action is advisory by default. It verifies the native release, writes the
health dashboard to the job summary, emits at most 10 annotations, and uploads
only `health.md` for 14 days. Enforcement, larger artifacts, and pull request
comments are explicit opt-ins. See [GitHub Action](docs/github-action.md).

## Command Map

| Command | Purpose |
| --- | --- |
| `git slop init` | Create repo-local config, ignore rules, and state directories |
| `git slop find` | Analyze the repository and write a fresh report bundle |
| `git slop health` | Render the human or CI health view from an existing report |
| `git slop show` | Inspect one file or folder record |
| `git slop explain` | Explain a path, relationship, cluster, or the top findings |
| `git slop plan` | Propose bounded maintenance slices from reviewed evidence |
| `git slop check` | Apply the stable detector gate |
| `git slop compare` | Compare two existing reports without rerunning analysis |
| `git slop sarif` | Export action-queue findings as SARIF 2.1.0 |
| `git slop version` | Print the installed version |
| `git slop build-info` | Print machine-readable package and source provenance |

See the [Command Guide](docs/commands.md) for selectors, formats, prompt packs,
CI thresholds, and examples.

## Working With Coding Agents

Git Slop does not require an LLM. When an agent handoff is useful, `explain`
and `plan` can write deterministic prompt packs containing bounded evidence and
explicit scope. The repository also ships a portable [Git Slop Agent
Plugin](plugins/git-slop/README.md) for installation, reporting, review,
planning, and adoption workflows.

## Trust Boundaries

The local Git Slop CLI does not:

- send repository data to a hosted service
- use an LLM to score files or change detector truth
- claim to detect AI authorship
- assign an overall code-quality grade
- treat a finding as proof that code is wrong
- rewrite source, tests, Git history, or GitHub state
- make autonomous refactoring decisions

These constraints are product architecture, not disclaimers. Read the
[Vision](docs/vision.md) for the deeper thesis and planned policy-guided advice
layer.

## Documentation

- [Vision](docs/vision.md)
- [Installation](docs/install.md)
- [Command Guide](docs/commands.md)
- [Report and Config Contract](docs/report-contract.md)
- [Scoring Model](docs/scoring-model.md)
- [Architecture](docs/architecture.md)
- [GitHub Action](docs/github-action.md)
- [Security Policy](SECURITY.md)
- [Contributing](CONTRIBUTING.md)
- [Brand Mark](assets/brand/README.md)

## Sponsors

Git Slop is supported by people and companies who want transparent,
local-first developer tooling to remain thoughtfully maintained. Sponsorship
funds releases, documentation, compatibility work, and ongoing maintenance.

[Become a sponsor](https://github.com/sponsors/coreycoto) or see
[Sponsors](SPONSORS.md) for current acknowledgments and the recognition policy.

---

🧑‍💻🤖🫟
