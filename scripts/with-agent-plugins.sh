#!/usr/bin/env bash
set -euo pipefail

repo_root="$(git rev-parse --show-toplevel)"
manifest_path="${repo_root}/.agents/plugins/marketplace-source.json"

source_url="$(jq -er '.source_url | select(type == "string" and length > 0)' "$manifest_path")"
source_revision="$(
  jq -er '.ref | select(type == "string" and test("^[0-9a-f]{40}$"))' "$manifest_path"
)"
agent_plugins_spec="agent-plugins @ git+${source_url}@${source_revision}"

exec uv run --no-project --with "$agent_plugins_spec" "$@"
