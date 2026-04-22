from __future__ import annotations

from importlib.metadata import PackageNotFoundError, version

__all__ = ["__version__"]


try:
    __version__ = version("git-slop")
except PackageNotFoundError:
    __version__ = "0.2.0"
