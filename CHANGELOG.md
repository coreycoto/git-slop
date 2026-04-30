# Changelog

All notable changes to `git-slop` are documented here.

This project follows semantic versioning for public CLI and artifact contracts.

## [Unreleased]

### Changed

- Prepared public repository docs, packaging metadata, and install language for
  open-source publication.

## [0.8.1] - 2026-04-30

### Fixed

- Fixed `git slop compare` when an overlay family is missing or `null` in one
  side of the comparison.

## [0.8.0] - 2026-04-30

### Added

- Additive V2 `explain` and `plan` payloads with stronger deterministic
  evidence summaries.
- Prompt-pack-only local model handoff for `explain` and `plan`.
- Preview-only backlog handoff metadata in plan JSON.
- Read-only report trend comparisons through `git slop compare`.
- SARIF 2.1.0 export through `git slop sarif`.
- Preview-only bounded refactor handoff through `git slop refactor-preview`.
- Editor-adjacent SARIF and JSON artifact consumption recipes.

### Changed

- Tuned Codex-powered governance workflows to reduce unnecessary model spend.

[Unreleased]: https://github.com/coreycoto/git-slop/compare/v0.8.1...HEAD
[0.8.1]: https://github.com/coreycoto/git-slop/releases/tag/v0.8.1
[0.8.0]: https://github.com/coreycoto/git-slop/releases/tag/v0.8.0
