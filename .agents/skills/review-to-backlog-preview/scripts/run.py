from __future__ import annotations

import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[4]
SRC_ROOT = REPO_ROOT / "src"
if str(SRC_ROOT) not in sys.path:
    sys.path.insert(0, str(SRC_ROOT))

from git_slop.agent_skill_runtime import run_skill_entrypoint  # noqa: E402

if __name__ == "__main__":
    raise SystemExit(
        run_skill_entrypoint(
            skill_name=Path(__file__).resolve().parents[1].name,
            script_path=__file__,
        )
    )
