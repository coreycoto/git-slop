# Installation

Homebrew is the supported end-user install path for Git Slop.

```bash
brew tap coreycoto/tap
brew install coreycoto/tap/git-slop
git-slop version
```

Upgrade with:

```bash
brew update
brew upgrade coreycoto/tap/git-slop
git-slop version
```

Consumer repositories should assume `git-slop` is installed on `PATH`, usually
through Homebrew. CI jobs that run Git Slop should install it the same way or
use an environment that already provides the executable.

Contributor setup is documented in [Contributing](../CONTRIBUTING.md).
