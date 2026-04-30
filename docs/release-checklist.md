# Git Slop Release Checklist

Use this checklist for semver releases that publish the CLI, Codex plugin, and
consumer install contracts.

## Prepare

- Confirm `pyproject.toml` has the intended version.
- Confirm the release tag does not already exist unless the release is being
  intentionally republished.
- Add release notes at `docs/releases/v<version>.md`. The tag-driven release
  workflow uses this file as the GitHub release notes when it exists and falls
  back to generated artifact-only notes otherwise.
- Run the local verification suite:

```bash
uv run ruff check
uv run pytest
```

## Build Locally

- Build wheel and source distributions:

```bash
uv build
```

- Create the local semver tag, then run the release preparation helper. The
  helper builds release artifacts, builds the release manifest with the exact
  tag SHA, and regenerates the private Homebrew tap formula:

```bash
git tag v<version>
uv run python scripts/release_prepare.py --version <version> --tap ../homebrew-tap
```

## Publish

- Push the semver tag and let `.github/workflows/release-publish.yml` publish
  the GitHub release artifacts:

```bash
git push origin v<version>
```

## Update Homebrew

- In `coreycoto/homebrew-tap`, verify:

```bash
brew style Formula/git-slop.rb
brew fetch --force coreycoto/tap/git-slop
brew reinstall coreycoto/tap/git-slop
brew test coreycoto/tap/git-slop
git-slop version
```

## Verify Consumers

- Keep release wheels available for `uv` and CI consumers.
- Update consumer pins only after the release assets and Homebrew formula are
  verified.
- For each pinned consumer, verify the wrapper can use an existing `git-slop`
  on `PATH` and can still fall back to the private release wheel when needed.

## Close Out

- Confirm the GitHub release contains the wheel, sdist, and
  `release-manifest.json`.
- Confirm release docs match the final install paths.
- Record any follow-up issues before moving to the next release.
