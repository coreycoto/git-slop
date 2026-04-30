# Git Slop Release Checklist

Use this checklist for semver releases that publish the CLI, Codex plugin, and
consumer install contracts.

## Prepare

- Confirm `pyproject.toml` has the intended version.
- Confirm the release tag does not already exist unless the release is being
  intentionally republished.
- Draft release notes in GitHub Releases after the workflow creates or updates
  the release. Do not commit per-release notes to the repository.
- Run the local verification suite:

```bash
uv run ruff check
uv run pytest
```

## Build Locally

- Build the package as a local packaging smoke:

```bash
uv build
```

- Create the local semver tag, then run the release preparation helper. The
  helper validates the package build, writes the Homebrew source manifest with
  the exact tag SHA, and regenerates the Homebrew tap formula:

```bash
git tag v<version>
uv run python scripts/release_prepare.py --version <version> --tap ../homebrew-tap
```

## Publish

- Push the semver tag and let `.github/workflows/release-publish.yml` create or
  update the GitHub Release notes:

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

- Keep consumer jobs on a `git-slop` executable from `PATH`, usually installed
  with Homebrew.
- Update consumer minimum-version checks only after the release and Homebrew
  formula are verified.
- For each consumer, verify the wrapper can use the installed executable.

## Close Out

- Confirm the GitHub Release exists for the tag.
- Confirm GitHub Release notes summarize the user-facing changes and release
  docs match the final install paths.
- Record any follow-up issues before moving to the next release.
