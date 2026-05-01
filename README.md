# 🧑‍💻🤖🫟 Git Slop

Find the files that cost too much context.

Git Slop is a deterministic, local-first detector for AI-era repositories. It
answers one stable question:

> Which files cost too much context to load, reason about, and safely change?

It also reports structural and operational evidence such as duplication,
scatter, weak verification, navigation friction, blast radius, stewardship
pressure, and semantic drift. Those overlays support review, but they do not
inflate the stable hotspot score.

## Install

```bash
brew tap coreycoto/tap
brew install coreycoto/tap/git-slop
git-slop version
```

See [Installation](docs/install.md) for install policy and contributor setup.

## Quick Start

```bash
git-slop init
git-slop find
git-slop show README.md
git-slop explain --top 5
git-slop plan --path src/git_slop
git-slop check
```

## Commands

- `git slop init`: create repo-local Git Slop config
- `git slop find`: analyze the current repo and write `.slop/latest/`
- `git slop show`: inspect one file from an existing report
- `git slop explain`: explain a file, folder, cluster, relationship, or top-N
- `git slop plan`: propose bounded maintenance slices from existing evidence
- `git slop check`: run the stable detector gate
- `git slop version`: print the installed version

The installed executable is `git-slop`. When it is on `PATH`, Git also accepts
`git slop`.

See [Command Guide](docs/commands.md) for options and examples.

## Boundaries

Git Slop does not:

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
- [Vision](docs/vision.md)
- [Release Checklist](docs/release-checklist.md)
- [Security Policy](SECURITY.md)
- [Contributing](CONTRIBUTING.md)

The `git-slop` Codex plugin is published from this repo. It owns
product-specific install, report, interpretation, planning, and
consumer-adoption guidance. Reusable project and backlog workflows live in the
separate `coreycoto/agent-plugins` plugin.
