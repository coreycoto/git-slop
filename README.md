# 🧑‍💻🤖🫟 Git Slop

Find the files that cost too much context.

Git Slop is a deterministic, local-first detector for AI-era repositories. It
answers one stable question:

> Which files cost too much context to load, reason about, and safely change?

It also reports structural and operational evidence such as duplication,
scatter, weak verification, navigation friction, blast radius, stewardship
pressure, and semantic drift. Those overlays support review, but they do not
inflate the stable hotspot score.

The public CLI is a native Rust executable. It needs Git, but it does not need
Python, a package-manager runtime, a hosted API, or a model provider.

## Philosophy

AI did not invent hard-to-maintain code. It made the cost of loading,
understanding, and safely changing a repository harder to ignore.

Git Slop is not an AI detector, a code-quality grade, or a judgment about who
wrote the code. It examines the repository that exists and asks how expensive
it is for a human or agent to work in.

- Measure maintenance pressure, not authorship.
- Prefer deterministic evidence over opaque judgments.
- Keep stable hotspot costs separate from supporting signals.
- Treat findings as prompts for investigation, not automatic verdicts.
- Propose bounded next steps; leave refactoring decisions to people.
- Stay local by default and keep repository data private.

The deeper product thesis and non-goals are documented in
[Vision](docs/vision.md).

## Install

After 0.9.0 is published, install its canonical crates.io package:

```bash
cargo install git-slop --version 0.9.0 --locked
git-slop build-info --format json
```

or with the Homebrew Formula:

```bash
brew tap coreycoto/tap
brew install coreycoto/tap/git-slop
git-slop version
git-slop build-info --format json
```

The Formula builds the exact checksummed crates.io source package; it is not a
cask. See [Installation](docs/install.md) for availability, provenance, and
contributor setup.

## GitHub Actions

Add a complete checkout and the Git Slop Action to get a detailed repository
health summary, bounded annotations, and a small review artifact:

```yaml
permissions:
  contents: read

steps:
  - uses: actions/checkout@v7
    with:
      fetch-depth: 0
  - uses: coreycoto/git-slop@v0.9.0
```

The Action is advisory by default. It verifies a prebuilt native binary built
from the exact crates.io package, writes `health.md` to the job summary, emits
at most 10 annotations, and uploads only `health.md` for 14 days. It never
installs through Homebrew. Enforcement, report-sized artifacts, and pull
request comments are explicit opt-ins. The same Action will be published in
GitHub Marketplace with the stable release. See [GitHub
Action](docs/github-action.md).

The `0.9.0` and `v0.9.0` examples describe the upcoming release contract. They
become installable only after the verified draft is published; this
documentation does not mean that 0.9.0 is already available.

## Quick Start

```bash
git-slop init
git-slop find
git-slop show README.md
git-slop explain --top 5
git-slop plan --path src
git-slop health
git-slop check
git-slop build-info --format json
```

`find` writes a complete bundle to `.slop/latest/` and a timestamped copy under
`.slop/runs/`:

- `report.json`: schema-4 automation contract
- `report.yaml`: equivalent machine data for YAML consumers
- `summary.md`: detailed detector and overlay evidence
- `health.md`: concise repository-health dashboard for humans and CI

## Commands

- `git slop init`: create repo-local Git Slop config
- `git slop find`: analyze the current repo and write `.slop/latest/`
- `git slop show`: inspect one file from an existing report
- `git slop explain`: explain a file, folder, cluster, relationship, or top-N
- `git slop plan`: propose bounded maintenance slices from existing evidence
- `git slop check`: run the stable detector gate
- `git slop compare`: compare two existing schema-4 reports
- `git slop sarif`: export action-queue findings as SARIF 2.1.0
- `git slop health`: render Markdown, GitHub annotations, or health JSON
- `git slop version`: print the installed version
- `git slop build-info`: print machine-readable package and source provenance

The installed executable is `git-slop`. When it is on `PATH`, Git also accepts
`git slop`.

See [Command Guide](docs/commands.md) for options and examples.

## Boundaries

The local `git-slop` CLI does not:

- rewrite code automatically
- require hosted APIs
- send repo data anywhere
- use an LLM for scoring
- treat detector findings as correctness proofs
- fold overlays into `slop_score`

## Documentation

- [Installation](docs/install.md)
- [Command Guide](docs/commands.md)
- [Report and Config Contract](docs/report-contract.md)
- [.slop Directory Policy](docs/slop-directory.md)
- [Architecture](docs/architecture.md)
- [Scoring Model](docs/scoring-model.md)
- [GitHub Action](docs/github-action.md)
- [Vision](docs/vision.md)
- [Release Checklist](docs/release-checklist.md)
- [Security Policy](SECURITY.md)
- [Contributing](CONTRIBUTING.md)

The planned 0.9.0 release moves the product runtime to Rust while retaining
report schema 4, config schema 2, and the existing `check` threshold semantics.
The repository uses a private, non-publishable Rust `xtask` workspace to
validate Codex, plugin, workflow, repository, and release wiring. It is
excluded from the public workspace and is not part of the native CLI or
`git-slop` Cargo package.

The `git-slop` Codex plugin is published from this repo. It owns
product-specific install, report, interpretation, planning, and
consumer-adoption guidance. Reusable project and backlog workflows live in the
separate `coreycoto/agent-plugins` plugin, which also owns its runtime behavior
tests, pinned marketplace bootstrap, and clean-room consumer smoke coverage.
When a maintainer workflow needs that private runtime, it acquires a
consumer-digest-pinned Linux PEX SCIE into an ephemeral job directory, verifies
its release metadata and embedded source revision, and then uses its direct CLI
through `scripts/with-agent-plugins.sh` without further publisher acquisition.
Its embedded marketplace installs offline; GitHub operations retain the
workflow token they need. Execution-state sync keeps its project PAT off runtime
verification, while privileged dependency remediation uses trusted base Codex
inputs and exposes the repository token only to its mutation step. The
acquisition token is not available during execution, and this repo carries no
Python project of its own. The public Git Slop release workflow is independent
of that private runtime.
