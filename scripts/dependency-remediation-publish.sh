#!/usr/bin/env bash
set -euo pipefail

export GIT_CONFIG_GLOBAL=/dev/null
export GIT_CONFIG_SYSTEM=/dev/null
export GIT_CONFIG_NOSYSTEM=1

program="${0##*/}"
fail() {
  printf '%s: error: %s\n' "$program" "$*" >&2
  exit 1
}
usage() {
  fail "usage: $program prepare|push|create-pr|verify-pr|continue PACKAGE_DIR"
}

[[ $# == 2 ]] || usage
command="$1"
package="$2"
[[ "$package" == /* && -d "$package" && ! -L "$package" ]] || fail "package must be an absolute, non-symlink directory"
package="$(cd -- "$package" && pwd -P)"
script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)"
trusted_root="${GH_STEWARD_TRUSTED_ROOT:?trusted control checkout is required}"
[[ "$(git -C "$trusted_root" rev-parse HEAD)" == "${GITHUB_WORKFLOW_SHA:?trusted workflow source SHA is required}" ]] || fail "trusted control checkout differs from the workflow source"
[[ -f "$script_dir/dependency-remediation-paths.sh" && ! -L "$script_dir/dependency-remediation-paths.sh" ]] || fail "trusted dependency-remediation path validator is missing"
# shellcheck source=scripts/dependency-remediation-paths.sh
source "$script_dir/dependency-remediation-paths.sh"
command -v jq >/dev/null 2>&1 || fail "jq is required"
command -v git >/dev/null 2>&1 || fail "git is required"
command -v gh >/dev/null 2>&1 || fail "gh is required"
[[ -n "${GH_STEWARD_BIN-}" && -x "$GH_STEWARD_BIN" ]] || fail "verified gh-steward binary is required"

context="$package/run-context.json"
intent="$package/publication/intent.json"
patch_file="$package/publication/patch.diff"
result_file="$package/publication/result.json"
event_file="$package/events/trigger-event.json"
candidate_file="$package/publication/candidate.json"
mkdir -m 0700 -p "$package/publication" "$package/events"
for file in "$context" "$result_file" "$event_file"; do
  [[ -f "$file" && ! -L "$file" ]] || fail "required trusted package file is missing or unsafe: ${file##*/}"
done

canonical_sha() {
  local file="$1" envelope
  [[ -f "$file" && ! -L "$file" ]] || fail "canonical digest input is missing or unsafe"
  envelope="$("$GH_STEWARD_BIN" runs digest --repo-root "$trusted_root" \
    --repo "${GITHUB_SERVER_URL%/}/$GITHUB_REPOSITORY" \
    --input "document=$file" --format json)" || fail "gh-steward could not compute the canonical document digest"
  jq -er 'select(.schema_version == 2) | .data.sha256 | select(type == "string" and test("^[a-f0-9]{64}$"))' \
    <<< "$envelope" || fail "gh-steward returned a malformed canonical document digest"
}

sha256_file() {
  shasum -a 256 "$1" | awk '{print $1}'
}

safe_repo_identity() {
  [[ "${GITHUB_REPOSITORY-}" =~ ^[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+$ ]] || fail "GITHUB_REPOSITORY is invalid"
  [[ "${GITHUB_SERVER_URL-}" =~ ^https://[^/]+/?$ ]] || fail "GITHUB_SERVER_URL must be an HTTPS origin"
  [[ "${GITHUB_RUN_ID-}" =~ ^[1-9][0-9]*$ && "${GITHUB_RUN_ATTEMPT-}" =~ ^[1-9][0-9]*$ ]] || fail "workflow run identity is invalid"
  [[ "${WORKFLOW_FILE-}" == "dependency-remediation.yml" ]] || fail "WORKFLOW_FILE must identify the trusted dependency-remediation workflow"
  [[ -n "${WORKFLOW_RUN_NAME-}" && "$WORKFLOW_RUN_NAME" != *$'\n'* && "$WORKFLOW_RUN_NAME" != *$'\r'* ]] || fail "WORKFLOW_RUN_NAME is invalid"
  [[ -n "${RECOVERY_KEY-}" && "$RECOVERY_KEY" != *$'\n'* && "$RECOVERY_KEY" != *$'\r'* ]] || fail "RECOVERY_KEY is invalid"
  [[ "${WORKFLOW_RUN_STARTED_AT-}" =~ ^[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2}Z$ ]] || fail "WORKFLOW_RUN_STARTED_AT is invalid"
}

sha40() {
  [[ "${1-}" =~ ^[0-9a-f]{40}$ ]]
}

positive_integer() {
  [[ "${1-}" =~ ^[1-9][0-9]*$ ]]
}

validate_files() {
  jq -e 'type == "object" and .status == "patched" and (.summary | type == "string" and length > 0) and (.title | type == "string" and length > 0 and length <= 256) and (.body | type == "string" and length > 0 and length <= 20000) and (.supplemental_patch | type == "string" and length > 0) and (.draft | type == "boolean") and (.verification | type == "array" and length > 0 and all(.[]; type == "string" and length > 0)) and (.changed_files | type == "array" and length > 0 and all(.[]; type == "string" and length > 0) and (unique | length) == length)' "$result_file" >/dev/null || fail "Codex result is not a verified patched result"
  [[ "$(jq -jr '.supplemental_patch' "$result_file" | shasum -a 256 | awk '{print $1}')" == "$(sha256_file "$patch_file")" ]] || fail "captured patch bytes differ from the exact Codex result"
  dependency_remediation_validate_result_paths "$result_file" || fail "Codex changed-file inventory contains an unsafe or out-of-scope path"
}

validate_git_refs() {
  local ref
  for ref in "$@"; do
    [[ -n "$ref" && "$ref" != *$'\n'* && "$ref" != *$'\r'* ]] || fail "a saved Git ref is empty or malformed"
    git check-ref-format "$ref" >/dev/null || fail "Git rejected saved ref $ref"
  done
}

validate_context() {
  local expected_repo="$GITHUB_REPOSITORY"
  jq -e --arg repo "$expected_repo" --arg workflow "$WORKFLOW_FILE" --arg key "$RECOVERY_KEY" --arg run_name "$WORKFLOW_RUN_NAME" \
    'type == "object" and (.schema_version | type == "number" and . == 1 and . == floor) and .repository == $repo and .workflow_file == $workflow and .recovery_key == $key and .run_name == $run_name and (.workflow_run_id | type == "number" and . > 0 and . == floor) and (.workflow_run_attempt | type == "number" and . > 0 and . == floor) and (.phase | IN("started", "prepared", "completed", "noop", "recovery_needed")) and (.plans | type == "array")' \
    "$context" >/dev/null || fail "run context does not match the exact repository and workflow"
}

validate_candidate() {
  [[ -f "$candidate_file" && ! -L "$candidate_file" ]] || fail "verified publication candidate is missing"
  local actual_event actual_patch actual_result expected_workflow_sha
  actual_event="$(sha256_file "$event_file")"
  actual_patch="$(sha256_file "$patch_file")"
  actual_result="$(sha256_file "$result_file")"
  expected_workflow_sha="${GITHUB_WORKFLOW_SHA:?trusted workflow SHA is missing}"
  [[ "$expected_workflow_sha" =~ ^[0-9a-f]{40}$ ]] || fail "trusted workflow SHA is malformed"
  jq -e \
    --arg repo "$GITHUB_REPOSITORY" --arg server "$GITHUB_SERVER_URL" \
    --arg workflow "$WORKFLOW_FILE" --arg event "$GITHUB_EVENT_NAME" \
    --arg event_sha "$actual_event" --arg patch_sha "$actual_patch" \
    --arg result_sha "$actual_result" --arg sha "$expected_workflow_sha" --arg live_event "${1-}" \
    --arg run "$GITHUB_RUN_ID" --arg attempt "$GITHUB_RUN_ATTEMPT" '
      type == "object"
      and (keys | sort) == ["base_ref","base_sha","candidate_tree_sha","event_name","origin_run_attempt","origin_run_id","patch_sha256","repository","result_sha256","schema_version","server_url","source_ref","source_repository","source_sha","trigger_event_sha256","upstream_artifacts","verification_job_name","workflow_file","workflow_sha"]
      and .schema_version == 1 and .repository == $repo and .server_url == $server
      and .workflow_file == $workflow and .event_name == $event
      and (.origin_run_id | type == "number" and . > 0 and . == floor)
      and (.origin_run_attempt | type == "number" and . > 0 and . == floor)
      and ($live_event != "live-event" or (.origin_run_id == ($run|tonumber) and .origin_run_attempt == ($attempt|tonumber) and .workflow_sha == $sha))
      and .trigger_event_sha256 == $event_sha
      and .patch_sha256 == $patch_sha and .result_sha256 == $result_sha
      and (.source_repository | type == "string" and length > 0)
      and (.source_ref | type == "string" and length > 0)
      and (.source_sha | type == "string" and test("^[0-9a-f]{40}$"))
      and (.base_ref | type == "string" and length > 0)
      and (.base_sha | type == "string" and test("^[0-9a-f]{40}$"))
      and (.candidate_tree_sha | type == "string" and test("^[0-9a-f]{40}$"))
      and .verification_job_name == "Verify publication candidate"
      and (.upstream_artifacts | type == "object" and (keys | sort) == ["capture","proposal"])
      and all(.upstream_artifacts[]; type == "object" and (keys | sort) == ["digest","id","name"] and (.id | type == "number" and . > 0 and . == floor) and (.name | type == "string" and length > 0) and (.digest | type == "string" and test("^sha256:[0-9a-f]{64}$")))
      and .upstream_artifacts.capture.name == ("gh-steward-dependency-remediation-capture-" + (.origin_run_id|tostring) + "-" + (.origin_run_attempt|tostring))
      and .upstream_artifacts.proposal.name == ("gh-steward-dependency-remediation-proposal-" + (.origin_run_id|tostring) + "-" + (.origin_run_attempt|tostring))
    ' "$candidate_file" >/dev/null || fail "candidate does not match the exact trusted workflow, trigger bytes, source, and immutable handoffs"
  local source_repository source_ref source_sha base_ref base_sha
  source_repository="$(jq -er '.source_repository' "$candidate_file")"
  source_ref="$(jq -er '.source_ref' "$candidate_file")"
  source_sha="$(jq -er '.source_sha' "$candidate_file")"
  base_ref="$(jq -er '.base_ref' "$candidate_file")"
  base_sha="$(jq -er '.base_sha' "$candidate_file")"
  [[ "$source_repository" == "$GITHUB_REPOSITORY" ]] || fail "candidate source is not in the exact target repository"
  validate_git_refs "refs/heads/$source_ref" "refs/heads/$base_ref"
  if [[ "${1-}" == "live-event" ]]; then
    [[ -n "${GITHUB_EVENT_PATH-}" && -f "$GITHUB_EVENT_PATH" ]] || fail "the live workflow event is unavailable"
    case "$GITHUB_EVENT_NAME" in
      pull_request_target)
        [[ "$(jq -er '.pull_request.head.repo.full_name' "$event_file")" == "$source_repository" \
          && "$(jq -er '.pull_request.head.ref' "$event_file")" == "$source_ref" \
          && "$(jq -er '.pull_request.head.sha' "$event_file")" == "$source_sha" \
          && "$(jq -er '.pull_request.base.ref' "$event_file")" == "$base_ref" \
          && "$(jq -er '.pull_request.base.sha' "$event_file")" == "$base_sha" ]] || fail "candidate source identity differs from the exact pull request event"
        [[ "$(jq -er '.pull_request.base.repo.full_name' "$event_file")" == "$GITHUB_REPOSITORY" ]] || fail "candidate base repository differs from the workflow repository"
        ;;
      schedule|workflow_dispatch)
        [[ "$source_sha" == "$GITHUB_SHA" && "$base_sha" == "$GITHUB_SHA" \
          && "$source_ref" == "$GITHUB_REF_NAME" && "$base_ref" == "$GITHUB_REF_NAME" ]] || fail "candidate source differs from the exact scheduled or manual event"
        ;;
      *) fail "candidate event type is unsupported" ;;
    esac
  fi
}

read_publication() {
  [[ -f "$intent" && ! -L "$intent" ]] || fail "immutable publication intent is missing"
  local actual expected
  actual="$(canonical_sha "$intent")"
  expected="$(jq -er '.publication.intent_sha256' "$context")"
  [[ "$actual" == "$expected" ]] || fail "immutable publication intent digest mismatch"
  validate_candidate
  jq -e --arg repo "$GITHUB_REPOSITORY" --arg workflow "$WORKFLOW_FILE" \
    --arg key "$RECOVERY_KEY" --arg run_name "$WORKFLOW_RUN_NAME" \
    --arg workflow_sha "$(jq -er '.workflow_sha' "$candidate_file")" \
    --arg candidate_sha "$(sha256_file "$candidate_file")" \
    --argjson candidate_run "$(jq -er '.origin_run_id' "$candidate_file")" \
    --argjson candidate_attempt "$(jq -er '.origin_run_attempt' "$candidate_file")" \
    --slurpfile candidate "$candidate_file" \
    'type == "object" and (.schema_version | type == "number" and . == 2 and . == floor) and .repository == $repo and .workflow_file == $workflow and .recovery_key == $key and .run_name == $run_name and .origin_run_id == $candidate_run and .origin_run_attempt == $candidate_attempt and .workflow_sha == $workflow_sha and .candidate_sha256 == $candidate_sha and .source_sha == $candidate[0].source_sha and .base_ref == $candidate[0].base_ref and .base_sha == $candidate[0].base_sha and .trigger_event_sha256 == $candidate[0].trigger_event_sha256 and .patch_sha256 == $candidate[0].patch_sha256 and .result_sha256 == $candidate[0].result_sha256 and (.origin_run_id | type == "number" and . > 0 and . == floor) and (.origin_run_attempt | type == "number" and . > 0 and . == floor) and (.mode | IN("update-existing-pr", "create-pr")) and (.stage == null)' \
    "$intent" >/dev/null || fail "publication intent does not match the exact workflow identity"
}

normalize_remote() {
  local url="$1" host repo
  host="${GITHUB_SERVER_URL#https://}"
  host="${host%/}"
  repo="$GITHUB_REPOSITORY"
  case "$url" in
    "https://$host/$repo"|"https://$host/$repo.git"|"http://$host/$repo"|"http://$host/$repo.git") return 0 ;;
    "git@$host:$repo"|"git@$host:$repo.git"|"ssh://git@$host/$repo"|"ssh://git@$host/$repo.git") return 0 ;;
    *) fail "git origin does not identify the exact workflow repository" ;;
  esac
}

remote_ref_sha() {
  local ref="$1" line
  line="$(git ls-remote --refs origin "$ref")" || fail "complete remote ref read failed"
  [[ -z "$line" ]] && return 0
  [[ "$(printf '%s\n' "$line" | wc -l | tr -d ' ')" == 1 ]] || fail "remote ref read returned duplicate identities"
  local actual_ref actual_sha
  read -r actual_sha actual_ref <<< "$line"
  [[ "$actual_ref" == "$ref" ]] && sha40 "$actual_sha" || fail "remote ref read returned a malformed identity"
  printf '%s' "$actual_sha"
}

recompute_commit() {
  local source_sha="$1" patch_sha="$2" expected_files="$3" created_at="$4" title="$5"
  local tree commit files_json
  sha40 "$source_sha" || fail "publication source commit is invalid"
  [[ "$(sha256_file "$patch_file")" == "$patch_sha" ]] || fail "publication patch digest mismatch"
  GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_SYSTEM=/dev/null GIT_CONFIG_NOSYSTEM=1 git fetch --no-tags origin "$source_sha" >/dev/null || fail "exact publication source commit could not be fetched"
  GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_SYSTEM=/dev/null GIT_CONFIG_NOSYSTEM=1 git checkout --detach "$source_sha" >/dev/null 2>&1 || fail "could not select the exact publication source commit"
  GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_SYSTEM=/dev/null GIT_CONFIG_NOSYSTEM=1 git -c core.hooksPath=/dev/null apply --cached --check --binary "$patch_file" || fail "captured patch no longer applies to its exact source commit"
  GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_SYSTEM=/dev/null GIT_CONFIG_NOSYSTEM=1 git -c core.hooksPath=/dev/null apply --cached --binary "$patch_file" || fail "captured patch could not be applied to the index"
  files_json="$(dependency_remediation_git_file_list_json "$source_sha")" || fail "changed file names are not valid UTF-8"
  jq -e --argjson expected "$expected_files" --argjson actual "$files_json" '$expected == $actual' >/dev/null || fail "patch paths differ from the captured dependency remediation plan"
  tree="$(git write-tree)" || fail "git could not construct the exact staged tree"
  commit="$(printf '%s\n\nPrepared by the trusted dependency remediation workflow.' "$title" | \
    GIT_AUTHOR_NAME="github-actions[bot]" GIT_AUTHOR_EMAIL="41898282+github-actions[bot]@users.noreply.github.com" \
    GIT_COMMITTER_NAME="github-actions[bot]" GIT_COMMITTER_EMAIL="41898282+github-actions[bot]@users.noreply.github.com" \
    GIT_AUTHOR_DATE="$created_at" GIT_COMMITTER_DATE="$created_at" \
    git -c core.hooksPath=/dev/null commit-tree "$tree" -p "$source_sha")" || fail "git could not construct the exact commit"
  sha40 "$commit" || fail "git returned an invalid commit identity"
  printf '%s' "$commit"
}

write_context() {
  local phase="$1" stage="$2" push_ack="$3" pr_ack="$4" verify_ack="$5" steps_json="$6"
  jq --arg phase "$phase" --arg stage "$stage" --argjson push_ack "$push_ack" --argjson pr_ack "$pr_ack" \
    --argjson verify_ack "$verify_ack" --argjson steps "$steps_json" \
    '.phase = $phase | .dispatch_steps = $steps | .publication.stage = $stage | .publication.push_ack = $push_ack | .publication.pr_ack = $pr_ack | .publication.verify_ack = $verify_ack' \
    "$context" > "$context.tmp" || fail "could not serialize exact dependency publication state"
  chmod 0600 "$context.tmp"
  mv -f "$context.tmp" "$context"
}

write_json_atomic() {
  local path="$1"; shift
  jq -n "$@" > "$path.tmp" || fail "could not serialize a durable dependency publication receipt"
  chmod 0600 "$path.tmp"
  mv -f "$path.tmp" "$path"
}

continue_publication() {
  if [[ ! -f "$intent" ]]; then
    "$0" prepare "$package"
  fi
  while :; do
    read_publication
    case "$(jq -er '.publication.stage' "$context")" in
      push-pending) "$0" push "$package" ;;
      pr-pending) "$0" create-pr "$package" ;;
      pr-verify-pending) "$0" verify-pr "$package" ;;
      completed) return 0 ;;
      *) fail "saved publication has an unsupported or ambiguous continuation stage" ;;
    esac
  done
}

safe_repo_identity
validate_context

case "$command" in
  continue)
    continue_publication
    ;;
  prepare)
    [[ -z "$(jq -r '.publication // empty' "$context")" ]] || fail "publication intent already exists; recovery must reuse it"
    validate_files
    [[ -f "$patch_file" && ! -L "$patch_file" ]] || fail "captured dependency patch is missing"
    validate_candidate live-event
    [[ -z "$(git status --porcelain)" ]] || fail "trusted prepare checkout must be clean before staging the captured patch"

    repo="$GITHUB_REPOSITORY"
    event_name="$GITHUB_EVENT_NAME"
    event_sha="$(sha256_file "$event_file")"
    [[ -n "${GITHUB_EVENT_PATH-}" && -f "$GITHUB_EVENT_PATH" ]] || fail "the live workflow event is unavailable"
    [[ "$(sha256_file "$GITHUB_EVENT_PATH")" == "$event_sha" ]] || fail "captured trigger bytes differ from the live event"
    result_sha="$(sha256_file "$result_file")"
    patch_sha="$(sha256_file "$patch_file")"
    source_repository="$repo"
    source_ref=""
    source_sha=""
    base_ref=""
    base_sha=""
    expected_old_sha="null"
    head_ref=""
    target_pr="null"
    mode="create-pr"
    title="$(jq -er '.title' "$result_file")"
    body="$(jq -er '.body' "$result_file")"
    draft="$(jq -er '.draft' "$result_file")"
    [[ "$title" != *$'\n'* && "$title" != *$'\r'* ]] || fail "pull request title contains a line break"
    publisher_login="$(gh api user --jq .login)" || fail "trusted publisher identity could not be read"
    repo_node_id="$(gh api "repos/$repo" --jq .node_id)" || fail "target repository identity could not be read"
    [[ -n "$publisher_login" && -n "$repo_node_id" ]] || fail "trusted publisher or repository identity is empty"

    case "$event_name" in
      pull_request_target)
        [[ "${GITHUB_ACTOR-}" == "dependabot[bot]" ]] || fail "pull_request_target remediation is limited to Dependabot"
        source_repository="$(jq -er '.pull_request.head.repo.full_name' "$event_file")"
        [[ "$source_repository" == "$repo" ]] || fail "fork dependency branches cannot receive a trusted push"
        pr_number="$(jq -er '.pull_request.number' "$event_file")"
        source_ref="refs/heads/$(jq -er '.pull_request.head.ref' "$event_file")"
        source_sha="$(jq -er '.pull_request.head.sha' "$event_file")"
        base_ref="$(jq -er '.pull_request.base.ref' "$event_file")"
        base_sha="$(jq -er '.pull_request.base.sha' "$event_file")"
        [[ "$(jq -er '.pull_request.base.repo.full_name' "$event_file")" == "$repo" ]] || fail "dependency PR base repository differs from the workflow repository"
        [[ "$source_repository" == "$(jq -er '.source_repository' "$candidate_file")" \
          && "$source_sha" == "$(jq -er '.source_sha' "$candidate_file")" \
          && "$base_ref" == "$(jq -er '.base_ref' "$candidate_file")" \
          && "$base_sha" == "$(jq -er '.base_sha' "$candidate_file")" ]] || fail "candidate and publisher source identities differ"
        sha40 "$source_sha" && sha40 "$base_sha" && positive_integer "$pr_number" || fail "dependency PR source identity is malformed"
        current_pr="$(gh api "repos/$repo/pulls/$pr_number")" || fail "dependency PR state could not be read"
        jq -e --arg repo "$repo" --arg ref "${source_ref#refs/heads/}" --arg sha "$source_sha" --arg base "$base_ref" --arg base_sha "$base_sha" \
          '.state == "open" and .head.repo.full_name == $repo and .head.ref == $ref and .head.sha == $sha and .base.repo.full_name == $repo and .base.ref == $base and .base.sha == $base_sha' \
          <<< "$current_pr" >/dev/null || fail "live Dependabot PR no longer matches the exact trusted event"
        expected_old_sha="$source_sha"
        head_ref="$source_ref"
        mode="update-existing-pr"
        validate_git_refs "$source_ref" "refs/heads/$base_ref"
        target_pr="$(jq -c --arg server "$GITHUB_SERVER_URL" --arg repo "$repo" '
          {number:.number,url:.html_url,head_repository:.head.repo.full_name,head_ref:.head.ref,head_sha:.head.sha,
           base_repository:.base.repo.full_name,base_ref:.base.ref,author_login:.user.login,title:.title,
           body:(.body // ""),draft:.draft}' <<< "$current_pr")"
        title="$(jq -r '.title' <<< "$target_pr")"
        body="$(jq -r '.body' <<< "$target_pr")"
        draft="$(jq -r '.draft' <<< "$target_pr")"
        ;;
      schedule|workflow_dispatch)
        default_branch="$(gh api "repos/$repo" --jq .default_branch)" || fail "default branch identity could not be read"
        base_ref="${GITHUB_REF_NAME:?workflow ref is missing}"
        [[ "$base_ref" == "$default_branch" ]] || fail "scheduled and manual remediation must run from the repository default branch"
        base_sha="$GITHUB_SHA"
        source_sha="$GITHUB_SHA"
        source_ref="refs/heads/$base_ref"
        head_ref="refs/heads/codex/dependency-remediation-$GITHUB_RUN_ID"
        expected_old_sha="null"
        validate_git_refs "$source_ref" "refs/heads/$base_ref" "$head_ref"
        [[ "$source_sha" == "$(jq -er '.source_sha' "$candidate_file")" \
          && "$base_ref" == "$(jq -er '.base_ref' "$candidate_file")" \
          && "$base_sha" == "$(jq -er '.base_sha' "$candidate_file")" ]] || fail "candidate and publisher source identities differ"
        ;;
      *) fail "unsupported dependency remediation event: $event_name" ;;
    esac

    normalize_remote "$(git remote get-url origin)"
    remote_source_sha="$(remote_ref_sha "$source_ref")"
    [[ "$remote_source_sha" == "$source_sha" ]] || fail "captured source branch moved before publication planning"
    remote_base_sha="$(remote_ref_sha "refs/heads/$base_ref")"
    [[ "$remote_base_sha" == "$base_sha" ]] || fail "captured base branch moved before publication planning"
    if [[ "$expected_old_sha" == "null" ]]; then
      [[ -z "$(remote_ref_sha "$head_ref")" ]] || fail "new dependency remediation branch already exists"
    else
      [[ "$(remote_ref_sha "$head_ref")" == "$expected_old_sha" ]] || fail "dependency PR branch moved before publication planning"
    fi

    source_sha_for_commit="$source_sha"
    expected_files="$(jq -cS '.changed_files | sort' "$result_file")"
    new_sha="$(recompute_commit "$source_sha_for_commit" "$patch_sha" "$expected_files" "${WORKFLOW_RUN_STARTED_AT:?run start timestamp is missing}" "$title")"
    candidate_tree="$(git rev-parse "$new_sha^{tree}")" || fail "publisher could not read its deterministic candidate tree"
    [[ "$candidate_tree" == "$(jq -er '.candidate_tree_sha' "$candidate_file")" ]] || fail "publisher candidate tree differs from the credential-free verification handoff"
    nonce="$(cat /proc/sys/kernel/random/uuid 2>/dev/null || uuidgen | tr '[:upper:]' '[:lower:]')"
    [[ "$nonce" =~ ^[0-9a-f]{8}-[0-9a-f]{4}-[1-5][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$ ]] || fail "could not create an exact publication nonce"
    files_json="$expected_files"
    jq -n -S --arg repo "$repo" --arg repo_node_id "$repo_node_id" --arg workflow "$WORKFLOW_FILE" \
      --arg recovery_key "$RECOVERY_KEY" --arg run_name "$WORKFLOW_RUN_NAME" \
      --arg workflow_sha "$(jq -er '.workflow_sha' "$candidate_file")" \
      --arg candidate_sha "$(sha256_file "$candidate_file")" \
      --argjson origin_run_id "$GITHUB_RUN_ID" --argjson origin_run_attempt "$GITHUB_RUN_ATTEMPT" \
      --arg event_name "$event_name" --arg event_sha "$event_sha" --arg source_repository "$source_repository" \
      --arg source_ref "$source_ref" --arg source_sha "$source_sha" --argjson target_pr "$target_pr" \
      --arg mode "$mode" --arg base_ref "$base_ref" --arg base_sha "$base_sha" --arg head_ref "$head_ref" \
      --argjson expected_old_sha "$expected_old_sha" --arg new_sha "$new_sha" --arg publisher "$publisher_login" \
      --arg title "$title" --arg body "$body" --argjson draft "$draft" --arg nonce "$nonce" \
      --arg patch_sha "$patch_sha" --arg result_sha "$result_sha" --argjson files "$files_json" \
      --arg created_at "$WORKFLOW_RUN_STARTED_AT" \
      '{schema_version:2,repository:$repo,repository_node_id:$repo_node_id,workflow_file:$workflow,recovery_key:$recovery_key,workflow_sha:$workflow_sha,candidate_sha256:$candidate_sha,
        run_name:$run_name,origin_run_id:$origin_run_id,origin_run_attempt:$origin_run_attempt,event_name:$event_name,
        trigger_event_sha256:$event_sha,source_repository:$source_repository,source_ref:$source_ref,source_sha:$source_sha,
        target_pr:$target_pr,mode:$mode,base_ref:$base_ref,base_sha:$base_sha,head_ref:$head_ref,
        expected_old_sha:$expected_old_sha,new_sha:$new_sha,publisher_login:$publisher,title:$title,body:$body,draft:$draft,
        nonce:$nonce,patch_sha256:$patch_sha,result_sha256:$result_sha,files:$files,created_at:$created_at}' \
      > "$intent.tmp" || fail "could not write exact dependency publication intent"
    chmod 0600 "$intent.tmp"
    mv -f "$intent.tmp" "$intent"
    intent_sha="$(canonical_sha "$intent")"
    jq -n -S --arg sha "$intent_sha" --argjson run "$GITHUB_RUN_ID" --argjson attempt "$GITHUB_RUN_ATTEMPT" \
      '{intent_sha256:$sha,origin_run_id:$run,origin_run_attempt:$attempt,stage:"push-pending",push_ack:null,pr_ack:null,verify_ack:null}' \
      > "$package/publication/context.json"
    jq --argjson publication "$(cat "$package/publication/context.json")" --argjson steps '["branch-push"]' \
      '.phase="prepared" | .dispatch_steps=$steps | .publication=$publication' "$context" > "$context.tmp"
    chmod 0600 "$context.tmp"
    mv -f "$context.tmp" "$context"
    printf 'prepared dependency publication %s\n' "$intent_sha"
    ;;

  push)
    read_publication
    [[ "$(jq -er '.publication.stage' "$context")" == "push-pending" ]] || fail "saved publication is not awaiting its branch push"
    [[ -n "${GH_TOKEN-}" ]] || fail "GH_TOKEN is required for the exact branch push"
    repository="$GITHUB_REPOSITORY"
    normalize_remote "$(git remote get-url origin)"
    repo_node_id="$(jq -er '.repository_node_id' "$intent")"
    source_sha="$(jq -er '.source_sha' "$intent")"
    source_ref="$(jq -er '.source_ref' "$intent")"
    base_ref="$(jq -er '.base_ref' "$intent")"
    base_sha="$(jq -er '.base_sha' "$intent")"
    head_ref="$(jq -er '.head_ref' "$intent")"
    new_sha="$(jq -er '.new_sha' "$intent")"
    expected_old="$(jq -r '.expected_old_sha // ""' "$intent")"
    patch_sha="$(jq -er '.patch_sha256' "$intent")"
    expected_files="$(jq -cS '.files' "$intent")"
    created_at="$(jq -er '.created_at' "$intent")"
    title="$(jq -er '.title' "$intent")"
    [[ "$(remote_ref_sha "refs/heads/$base_ref")" == "$base_sha" ]] || fail "base branch changed after publication preparation"
    actual_old="$(remote_ref_sha "$head_ref")"
    if [[ -n "$expected_old" ]]; then
      [[ "$actual_old" == "$expected_old" && "$expected_old" == "$source_sha" ]] || fail "branch ref no longer matches the exact saved lease"
    else
      [[ -z "$actual_old" && "$source_sha" == "$base_sha" ]] || fail "new branch is no longer absent or source differs from base"
    fi
    validate_git_refs "$source_ref" "refs/heads/$base_ref" "$head_ref"
    if [[ "$(jq -er '.mode' "$intent")" == "update-existing-pr" ]]; then
      pr_number="$(jq -er '.target_pr.number' "$intent")"
      current_pr="$(gh api "repos/$repository/pulls/$pr_number")" || fail "exact Dependabot PR could not be re-read before branch publication"
      target_pr="$(jq -c '.target_pr' "$intent")"
      jq -e --argjson target "$target_pr" --arg repo "$repository" --arg ref "${source_ref#refs/heads/}" --arg sha "$source_sha" --arg base "$base_ref" --arg base_sha "$base_sha" \
        '{number:.number,url:.html_url,head_repository:.head.repo.full_name,head_ref:.head.ref,head_sha:.head.sha,base_repository:.base.repo.full_name,base_ref:.base.ref,author_login:.user.login,title:.title,body:(.body // ""),draft:.draft} == $target and .state == "open" and .head.repo.full_name == $repo and .head.ref == $ref and .head.sha == $sha and .base.repo.full_name == $repo and .base.ref == $base and .base.sha == $base_sha' \
        <<< "$current_pr" >/dev/null || fail "live Dependabot PR changed since exact publication preparation"
    fi
    recomputed="$(recompute_commit "$source_sha" "$patch_sha" "$expected_files" "$created_at" "$title")"
    [[ "$recomputed" == "$new_sha" ]] || fail "deterministic commit differs from the exact prepared publication plan"
    push_output="$package/publication/push-output.txt"
    if ! git -c core.hooksPath=/dev/null -c credential.helper= -c credential.helper='!gh auth git-credential' \
      push --porcelain --force-with-lease="$head_ref:$expected_old" origin "$new_sha:$head_ref" > "$push_output"; then
      fail "git push did not return a positive acknowledgement; do not replay this publication"
    fi
    write_json_atomic "$package/publication/push-ack.json" \
      --arg nonce "$(jq -er '.nonce' "$intent")" --arg repo "$repository" --arg node "$repo_node_id" \
      --arg ref "$head_ref" --arg new "$new_sha" --argjson old "$(jq -c '.expected_old_sha' "$intent")" \
      '{schema_version:1,nonce:$nonce,repository:$repo,repository_node_id:$node,ref:$ref,expected_old_sha:$old,new_sha:$new,positive_push_ack:true}'
    push_ack="$(cat "$package/publication/push-ack.json")"
    mode="$(jq -er '.mode' "$intent")"
    if [[ "$mode" == "create-pr" ]]; then
      stage="pr-pending"
      steps='["pull-request-create"]'
    else
      stage="pr-verify-pending"
      steps='[]'
    fi
    write_context prepared "$stage" "$push_ack" null null "$steps"
    printf 'positive branch push acknowledgement recorded for %s\n' "$head_ref"
    ;;

  create-pr)
    read_publication
    [[ "$(jq -er '.publication.stage' "$context")" == "pr-pending" ]] || fail "saved publication is not awaiting PR creation"
    [[ -n "${GH_TOKEN-}" ]] || fail "GH_TOKEN is required for exact PR creation"
    repo="$GITHUB_REPOSITORY"
    node_id="$(jq -er '.repository_node_id' "$intent")"
    head_ref="$(jq -er '.head_ref' "$intent")"
    head_branch="${head_ref#refs/heads/}"
    base_ref="$(jq -er '.base_ref' "$intent")"
    new_sha="$(jq -er '.new_sha' "$intent")"
    title="$(jq -er '.title' "$intent")"
    body="$(jq -er '.body' "$intent")"
    draft="$(jq -er '.draft' "$intent")"
    live_repo_node_id="$(gh api "repos/$repo" --jq .node_id)" || fail "target repository immutable identity could not be re-read before PR dispatch"
    [[ "$live_repo_node_id" == "$node_id" ]] || fail "target repository identity changed before PR publication"
    validate_git_refs "refs/heads/$base_ref" "refs/heads/$head_branch"
    [[ "$(remote_ref_sha "refs/heads/$base_ref")" == "$(jq -er '.base_sha' "$intent")" ]] || fail "base branch changed after branch publication"
    [[ "$(remote_ref_sha "$head_ref")" == "$new_sha" ]] || fail "branch acknowledgement cannot be matched to the exact remote commit"
    open_prs="$(gh pr list --repo "$repo" --state open --head "$head_branch" --base "$base_ref" --json number,url)" || fail "open PR inventory could not be read"
    jq -e 'type == "array" and length == 0' <<< "$open_prs" >/dev/null || fail "an open PR appeared after exact create intent; do not create or adopt it"
    create_output="$package/publication/create-pr-output.txt"
    create_args=(pr create --repo "$repo" --head "$head_branch" --base "$base_ref" --title "$title" --body "$body")
    [[ "$draft" == true ]] && create_args+=(--draft)
    if ! gh "${create_args[@]}" > "$create_output"; then
      fail "gh pr create did not return a positive acknowledgement; do not replay PR creation"
    fi
    url="$(tr -d '\r\n' < "$create_output")"
    server="${GITHUB_SERVER_URL%/}"
    prefix="$server/$repo/pull/"
    [[ "$url" == "$prefix"* ]] || fail "gh pr create stdout did not contain one exact same-repository PR URL"
    number="${url#"$prefix"}"
    positive_integer "$number" && [[ "$url" == "$prefix$number" ]] || fail "gh pr create stdout did not contain one exact same-repository PR URL"
    write_json_atomic "$package/publication/pr-ack.json" \
      --arg nonce "$(jq -er '.nonce' "$intent")" --arg repo "$repo" --arg node "$node_id" \
      --arg url "$url" --arg head "$head_branch" --arg new "$new_sha" --arg base "$base_ref" \
      --arg base_sha "$(jq -er '.base_sha' "$intent")" --arg publisher "$(jq -er '.publisher_login' "$intent")" \
      --arg title "$title" --arg body "$body" --argjson draft "$draft" --argjson number "$number" \
      '{schema_version:1,nonce:$nonce,repository:$repo,repository_node_id:$node,url:$url,number:$number,
        head_ref:$head,head_sha:$new,base_ref:$base,base_sha:$base_sha,publisher_login:$publisher,title:$title,body:$body,draft:$draft}'
    pr_ack="$(cat "$package/publication/pr-ack.json")"
    write_context prepared pr-verify-pending "$(jq -c '.publication.push_ack' "$context")" "$pr_ack" null '[]'
    printf 'positive PR creation acknowledgement recorded for %s\n' "$url"
    ;;

  verify-pr)
    read_publication
    [[ "$(jq -er '.publication.stage' "$context")" == "pr-verify-pending" ]] || fail "saved publication is not awaiting read-only PR verification"
    [[ -n "${GH_TOKEN-}" ]] || fail "GH_TOKEN is required for exact PR verification"
    repo="$GITHUB_REPOSITORY"
    repo_node_id="$(gh api "repos/$repo" --jq .node_id)" || fail "target repository identity could not be independently read"
    [[ "$repo_node_id" == "$(jq -er '.repository_node_id' "$intent")" ]] || fail "target repository immutable identity changed"
    if [[ "$(jq -r '.publication.pr_ack' "$context")" != "null" ]]; then
      pr_number="$(jq -er '.publication.pr_ack.number' "$context")"
      expected_url="$(jq -er '.publication.pr_ack.url' "$context")"
      expected_author="$(jq -er '.publisher_login' "$intent")"
      expected_title="$(jq -er '.title' "$intent")"
      expected_body="$(jq -er '.body' "$intent")"
      expected_draft="$(jq -er '.draft' "$intent")"
    else
      pr_number="$(jq -er '.target_pr.number' "$intent")"
      expected_url="$(jq -er '.target_pr.url' "$intent")"
      expected_author="$(jq -er '.target_pr.author_login' "$intent")"
      expected_title="$(jq -er '.target_pr.title' "$intent")"
      expected_body="$(jq -r '.target_pr.body' "$intent")"
      expected_draft="$(jq -er '.target_pr.draft' "$intent")"
    fi
    verified_pr="$(gh pr view "$pr_number" --repo "$repo" --json number,url,headRefName,headRefOid,baseRefName,author,title,body,isDraft,state)" || fail "exact dependency PR could not be read after publication"
    jq -e --arg repo "$repo" --arg url "$expected_url" --arg head "$(jq -er '.head_ref' "$intent" | sed 's#^refs/heads/##')" \
      --arg sha "$(jq -er '.new_sha' "$intent")" --arg base "$(jq -er '.base_ref' "$intent")" \
      --arg author "$expected_author" --arg title "$expected_title" --arg body "$expected_body" --argjson draft "$expected_draft" \
      '.state == "OPEN" and .url == $url and .headRefName == $head and .headRefOid == $sha and .baseRefName == $base and .author.login == $author and .title == $title and (.body // "") == $body and .isDraft == $draft' \
      <<< "$verified_pr" >/dev/null || fail "live PR differs from the exact saved publication intent"
    write_json_atomic "$package/publication/verify-ack.json" \
      --arg nonce "$(jq -er '.nonce' "$intent")" --arg repo "$repo" --arg node "$repo_node_id" \
      --arg url "$expected_url" --arg head "$(jq -er '.head_ref' "$intent" | sed 's#^refs/heads/##')" \
      --arg sha "$(jq -er '.new_sha' "$intent")" --arg base "$(jq -er '.base_ref' "$intent")" \
      --arg author "$expected_author" --arg title "$expected_title" --arg body "$expected_body" \
      --argjson number "$pr_number" --argjson draft "$expected_draft" \
      '{schema_version:1,nonce:$nonce,repository:$repo,repository_node_id:$node,url:$url,number:$number,
        head_ref:$head,head_sha:$sha,base_ref:$base,author_login:$author,title:$title,body:$body,draft:$draft,state:"OPEN",verified:true}'
    write_context completed completed "$(jq -c '.publication.push_ack' "$context")" \
      "$(jq -c '.publication.pr_ack' "$context")" "$(cat "$package/publication/verify-ack.json")" '[]'
    jq -n --arg sha "$(jq -er '.publication.intent_sha256' "$context")" --arg url "$expected_url" --argjson number "$pr_number" \
      '{schema_version:1,status:"completed",intent_sha256:$sha,url:$url,number:$number,mutations_replayed:false}' \
      > "$package/apply-result.json"
    printf 'verified exact dependency PR %s\n' "$expected_url"
    ;;
  *) usage ;;
esac
