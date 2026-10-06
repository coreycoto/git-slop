#!/usr/bin/env bash

dependency_remediation_path_allowed() {
  local path="$1" component remaining
  [[ -n "$path" && "$path" != /* && "$path" != *$'\\'* && "$path" != */ && "$path" != *//* ]] || return 1
  remaining="$path"
  while [[ "$remaining" == */* ]]; do
    component="${remaining%%/*}"
    [[ -n "$component" && "$component" != . && "$component" != .. ]] || return 1
    remaining="${remaining#*/}"
  done
  [[ -n "$remaining" && "$remaining" != . && "$remaining" != .. ]] || return 1
  case "$path" in
    src/*|tests/*|action/*|tools/*) return 0 ;;
    Cargo.toml|Cargo.lock|rust-toolchain.toml|package.json|package-lock.json|pnpm-lock.yaml|yarn.lock|bun.lock|bun.lockb|go.mod|go.sum|pyproject.toml|uv.lock|requirements.txt|requirements-*.txt|Gemfile|Gemfile.lock) return 0 ;;
    */Cargo.toml|*/Cargo.lock|*/package.json|*/package-lock.json|*/pnpm-lock.yaml|*/yarn.lock|*/bun.lock|*/bun.lockb|*/go.mod|*/go.sum|*/pyproject.toml|*/uv.lock|*/requirements.txt|*/requirements-*.txt|*/Gemfile|*/Gemfile.lock)
      case "$path" in action/*|tools/*|src/*|tests/*) return 0 ;; esac
      return 1
      ;;
    *) return 1 ;;
  esac
}

dependency_remediation_validate_result_paths() {
  local result_file="$1" path
  jq -e '.changed_files | type == "array" and length > 0 and all(.[]; type == "string" and length > 0) and (unique | length) == length' \
    "$result_file" >/dev/null || return 1
  while IFS= read -r -d '' path; do
    dependency_remediation_path_allowed "$path" || return 1
  done < <(jq -jr '.changed_files[] | ., "\u0000"' "$result_file")
}

dependency_remediation_git_file_list_json() {
  local source_sha="$1"
  git diff --cached --name-only -z "$source_sha" | jq -Rs 'split("\u0000") | map(select(length > 0)) | sort'
}
