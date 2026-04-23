from __future__ import annotations

import sys
from pathlib import Path


def _has_cached_official_github_plugin() -> bool:
    plugin_root = Path.home() / ".codex" / "plugins" / "cache" / "openai-curated" / "github"
    if not plugin_root.exists():
        return False
    return any((plugin_dir / ".codex-plugin" / "plugin.json").exists() for plugin_dir in plugin_root.iterdir())


def main() -> int:
    if _has_cached_official_github_plugin():
        print("Official GitHub Codex plugin detected.")
        return 0
    print(
        "Official GitHub Codex plugin not detected. Install and enable it before using "
        "the reusable project-management plugin for local interactive GitHub workflows.",
        file=sys.stderr,
    )
    return 1


if __name__ == "__main__":
    raise SystemExit(main())
