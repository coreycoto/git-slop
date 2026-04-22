from __future__ import annotations

import hashlib
import json
import shutil
from pathlib import Path
from typing import Any


def json_fingerprint(payload: Any) -> str:
    encoded = json.dumps(payload, sort_keys=True, separators=(",", ":")).encode("utf-8")
    return hashlib.sha256(encoded).hexdigest()


def read_json(path: Path) -> Any | None:
    if not path.exists():
        return None
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except json.JSONDecodeError:
        return None


def write_json(path: Path, payload: Any) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(
        json.dumps(payload, indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )


def write_atomic_json(path: Path, payload: Any) -> None:
    temp_path = path.with_name(f".{path.name}.tmp")
    write_json(temp_path, payload)
    temp_path.replace(path)


def load_or_compute_json(path: Path, builder) -> Any:
    cached = read_json(path)
    if cached is not None:
        return cached
    payload = builder()
    write_atomic_json(path, payload)
    return payload


def ensure_clean_dir(path: Path) -> None:
    shutil.rmtree(path, ignore_errors=True)
    path.mkdir(parents=True, exist_ok=True)
