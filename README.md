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
git slop find
git slop explain --top 5
```

`find` performs the analysis once, prints the concise health view, and keeps its
first-run state under Git-private storage when the repository has not adopted
Git Slop. The other commands discover that report without a path and without
rescoring it. Use `git slop health --format markdown` when you want to re-render
the full dashboard. Run from a full-history checkout when age, churn, coupling,
and stewardship evidence matter.

When the team wants durable repo-owned configuration and report state, adopt it
explicitly:

```bash
git slop init
git add .slop/config.yaml .slop/.gitignore
git slop find
```

`--ephemeral` remains available for an explicitly disposable scan even after
adoption; it is not needed for the ordinary no-adoption first look.

The health dashboard points to the next useful command. After reviewing a
finding, ask Git Slop for a bounded maintenance proposal:

```bash
git slop explain --path src/example.rs
git slop plan --path src/example.rs
```

A plan is evidence for human review. It does not edit code, invoke a model, or
mutate Git or GitHub.

When a separately operated local Safeguard endpoint is available, the optional
advisor can evaluate those deterministic candidates against inspectable policy
packs without changing detector truth:

```bash
git slop policy show core
git slop advise --top 1 --context-only --format json
```

Model inference is always explicit. See [Policy Packs](docs/policy-packs.md),
the [Policy-Guided Advisor](docs/advisor.md), and the
[privacy-safe Safeguard benchmark](docs/benchmarks/safeguard-v1.md).

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

Every successful `find` writes three required files plus optional YAML to
`.slop/latest/` and an immutable timestamped copy under `.slop/runs/`:

| Artifact | Purpose |
| --- | --- |
| `report.json` | Versioned machine contract for automation |
| `report.yaml` | Optional compatibility data when `output.yaml: true` |
| `summary.md` | Detailed detector and overlay evidence |
| `health.md` | Concise repository-health dashboard for people and CI |

Routine generated output stays untracked. Commit `.slop/config.yaml` and
`.slop/.gitignore` when a repository intentionally adopts Git Slop; see the
[`.slop` directory policy](docs/slop-directory.md).

## Install

The examples below pin the 0.16.0 release identity. Use each command only after
that exact version is published on the requested distribution surface;
documentation or a source tag is not proof that every surface is available.

See [Installation](docs/install.md) for availability, provenance, upgrades,
and contributor setup.

### Homebrew (macOS and Linux)

```bash
brew install coreycoto/tap/git-slop
```

### Cargo (Crates.io)

```bash
cargo install git-slop --version 0.16.0 --locked
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
  - uses: coreycoto/git-slop@v0.16.0
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
| `git slop report` | Validate a report or print the report JSON Schema |
| `git slop health` | Render the human or CI health view from an existing report |
| `git slop show` | Inspect one file or folder record |
| `git slop explain` | Explain a path, relationship, cluster, or the top findings |
| `git slop plan` | Propose bounded maintenance slices from reviewed evidence |
| `git slop policy` | Author, validate, install, lock, inspect, test, or remove data-only policy packs |
| `git slop advise` | Optionally evaluate deterministic candidates with locked policies and an explicit local Safeguard endpoint |
| `git slop check` | Apply the stable detector gate |
| `git slop compare` | Compare two existing reports without rerunning analysis |
| `git slop baseline` | Create, inspect, update, validate, or safely remove named baselines |
| `git slop sarif` | Export action-queue findings as SARIF 2.1.0 |
| `git slop config` | Inspect, validate, migrate, or describe configuration |
| `git slop doctor` | Diagnose repository readiness and resource estimates |
| `git slop list` | List policy failures, interventions, observations, advisory health findings, relationships, clusters, or profiles |
| `git slop prune` | Preview or remove retained immutable run snapshots |
| `git slop cache` | Inspect or prune the packed token cache |
| `git slop completions` | Generate completion source from the live command tree |
| `git slop man` | Generate the roff manual from the live command tree |
| `git slop reference` | Generate the Markdown CLI reference from the live command tree |
| `git slop schema` | Print a published machine-contract schema |
| `git slop html` | Write a self-contained local report browser |
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

The optional advisor is separate: it sends only bounded, provenance-rich
context to an explicitly configured endpoint, validates every reference, and
writes non-mutating artifacts outside the canonical report bundle.

Start with `git slop doctor`; see [Troubleshooting](docs/troubleshooting.md),
[Configuration Recipes](docs/config-recipes.md), the neutral [Worked
Example](docs/worked-example.md), and the [0.16.0 release notes](CHANGELOG.md).

## Trust Boundaries

The deterministic Git Slop commands do not:

- send repository data to a hosted service
- use an LLM to score files or change detector truth; `advise` is an explicit
  separate evaluator and remains advisory
- claim to detect AI authorship
- assign an overall code-quality grade
- treat a finding as proof that code is wrong
- rewrite source, tests, Git history, or GitHub state
- make autonomous refactoring decisions

These constraints are product architecture, not disclaimers. Read the
[Vision](docs/vision.md) for the deeper thesis and policy-guided advice
boundary. Distribution verification begins from the independent roots documented
in the [release trust graph](docs/release-trust.md).

## Documentation

- [Vision](docs/vision.md)
- [Installation](docs/install.md)
- [Command Guide](docs/commands.md)
- [Configuration Recipes](docs/config-recipes.md)
- [Troubleshooting](docs/troubleshooting.md)
- [Worked Example](docs/worked-example.md)
- [Named Baselines](docs/baselines.md)
- [Editor-Adjacent Workflows](docs/editor-integrations.md)
- [Report and Config Contract](docs/report-contract.md)
- [.slop Directory Policy](docs/slop-directory.md)
- [Scoring Model](docs/scoring-model.md)
- [Architecture](docs/architecture.md)
- [Policy Packs](docs/policy-packs.md)
- [Policy-Guided Advisor](docs/advisor.md)
- [GitHub Action](docs/github-action.md)
- [Release Checklist](docs/release-checklist.md)
- [Release Trust Graph](docs/release-trust.md)
- [Changelog](CHANGELOG.md)
- [Security Policy](SECURITY.md)
- [Contributing](CONTRIBUTING.md)
- [Agent Plugin Client Recipes](plugins/git-slop/CLIENTS.md)
- [Brand Mark](assets/brand/README.md)

## Sponsors

Git Slop is supported by people and companies who want transparent,
local-first developer tooling to remain thoughtfully maintained. Sponsorship
funds releases, documentation, compatibility work, and ongoing maintenance.

[Become a sponsor](https://github.com/sponsors/coreycoto) or see
[Sponsors](SPONSORS.md) for current acknowledgments and the recognition policy.

---

🧑‍💻🤖🫟
