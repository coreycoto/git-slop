<h1>
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="assets/brand/git-slop-inverse.svg">
    <img src="assets/brand/git-slop.svg" alt="" width="32" height="32">
  </picture>
  Git Slop
</h1>

Find the files that cost too much context.

Git Slop is a deterministic, local-first detector for AI-era repositories. It
answers one stable question:

> Which files cost too much context to load, reason about, and safely change?

It also reports structural and operational evidence such as duplication,
scatter, weak verification, navigation friction, blast radius, stewardship
pressure, and semantic drift. Those overlays support review, but they do not
inflate the stable hotspot score.

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

## Sponsors

Git Slop is supported by people and companies who want transparent,
local-first developer tooling to remain thoughtfully maintained. Sponsorship
funds releases, documentation, compatibility work, and ongoing maintenance.

[Become a sponsor](https://github.com/sponsors/coreycoto) or see
[Sponsors](SPONSORS.md) for current acknowledgments and the recognition policy.

### Founding company sponsors

_No active founding company sponsors yet._

### Company sponsors

_No active company sponsors yet._

Sponsorship provides recognition and a feedback channel. It does not include a
support SLA, guaranteed issue priority, or roadmap control.

## Install

The examples below pin the 0.9.5 release identity. Use each command only after
that exact version is published on the requested distribution surface;
documentation or a source tag is not proof that every surface is available.

After crates.io lists 0.9.5, install the canonical package:

```bash
cargo install git-slop --version 0.9.5 --locked
git-slop build-info --format json
```

After the tap lists 0.9.5, install the Homebrew Formula:

```bash
brew tap coreycoto/tap
brew install coreycoto/tap/git-slop
git-slop version
git-slop build-info --format json
```

The Formula builds the exact checksummed crates.io source package; it is not a
cask.

After the external Scoop bucket lists 0.9.5, Windows users can install the
matching checksummed native archive:

```powershell
scoop bucket add coreycoto https://github.com/coreycoto/scoop-bucket
scoop install coreycoto/git-slop
git-slop version
git-slop build-info --format json
git slop version
```

See [Installation](docs/install.md) for availability, provenance, upgrades,
and contributor setup.

## GitHub Actions

After the public GitHub Release and Marketplace listing resolve `v0.9.5`, add a
complete checkout and the Git Slop Action to get a detailed repository health
summary, bounded annotations, and a small review artifact:

```yaml
permissions:
  contents: read

steps:
  - uses: actions/checkout@v7
    with:
      fetch-depth: 0
  - uses: coreycoto/git-slop@v0.9.5
```

The Action is advisory by default. It verifies a prebuilt native binary built
from the exact crates.io package, writes `health.md` to the job summary, emits
at most 10 annotations, and uploads only `health.md` for 14 days. It never
installs through Homebrew. Enforcement, report-sized artifacts, and pull
request comments are explicit opt-ins. The same Action will be published in
GitHub Marketplace with the stable release. See [GitHub
Action](docs/github-action.md).

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

- [Brand mark](assets/brand/README.md)
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

The current release retains report schema 4, config schema 2, and the existing
`check` threshold semantics. A private, non-publishable Rust `xtask`
workspace validates Codex, plugin, workflow, repository, distribution, and
release wiring. It is excluded from the public workspace and the `git-slop`
Cargo package.

The `git-slop` Codex plugin is published from this repo. It owns
product-specific install, report, interpretation, planning, and
consumer-adoption guidance. Reusable project and backlog workflows live in the
separate `coreycoto/agent-plugins` plugin, which also owns its runtime behavior
tests, pinned marketplace bootstrap, and clean-room consumer smoke coverage.
Maintainer workflows acquire and verify that pinned prebuilt runtime in an
ephemeral job directory before invoking its direct CLI through
`scripts/with-agent-plugins.sh`. Acquisition credentials remain step-scoped,
and the public Git Slop release workflow is independent of the private runtime.

🧑‍💻🤖🫟
