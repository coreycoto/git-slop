#!/usr/bin/env bash
set -euo pipefail

args=(verify-changed)
if [[ -n "${VERIFY_CHANGED_BASE:-}" ]]; then
  args+=(--base "$VERIFY_CHANGED_BASE")
fi
exec cargo xtask "${args[@]}"
