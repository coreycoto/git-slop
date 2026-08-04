# Git Slop Plugin

This plugin is the Codex guidance surface for the `git-slop` product CLI.

It covers:

- installing and updating the native Rust CLI
- running report, health, explain, plan, and check commands
- interpreting `.slop/latest/` artifacts
- preserving `.slop/` generated-state boundaries
- planning bounded maintenance work from hotspot evidence
- adopting `git-slop` locally and through its GitHub Action

It intentionally does not own generic backlog, release, project, or governance
workflows. When a reviewed `git-slop plan` should become backlog work, use the
separate `project-management-workflows` plugin from `coreycoto/agent-plugins`.

The public `git-slop` runtime is a native executable and does not require
Python. `find` writes schema-4 JSON/YAML plus detailed `summary.md` and
CI-oriented `health.md`. Stable costs drive the existing `check` gate; overlays
and health rollups remain additive evidence.

For the `0.9.0` distribution contract, the published crates.io package is the
canonical source identity. The Homebrew Formula installs that exact crate, with
bottles serving only as faster transport, while the public Action installs
prebuilt GitHub Release archives verifiably bound to the same crate and source
revision. Until the crates.io package, matching release assets, and tap update
are each actually published, describe availability only for the verified
surface and keep the remaining `0.9.0` paths pending. Verify a published binary
with `git-slop build-info --format json`.

Product guidance should treat `.slop/latest/`, `.slop/runs/`, `.slop/cache/`,
prompt packs, SARIF exports, plan JSON, and compare JSON as generated artifacts
unless a repository intentionally curates them as fixtures outside `.slop/`.
The GitHub Action uploads only an allowlisted subset, with `health.md` as its
default artifact.
