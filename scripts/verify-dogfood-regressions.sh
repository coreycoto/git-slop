#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 5 ]]; then
  echo "usage: verify-dogfood-regressions.sh MANIFEST COMPARISON HEAD_REPORT BASE_SHA HEAD_SHA" >&2
  exit 2
fi

manifest=$1
comparison=$2
head_report=$3
base_sha=$4
head_sha=$5

for required in "$manifest" "$comparison" "$head_report"; do
  if [[ ! -f "$required" ]]; then
    echo "dogfood regression verification is missing a required input" >&2
    exit 2
  fi
done

if [[ ! $base_sha =~ ^[0-9a-f]{40}$ || ! $head_sha =~ ^[0-9a-f]{40}$ ]]; then
  echo "dogfood regression verification requires exact lowercase base and head SHAs" >&2
  exit 2
fi

scratch=$(mktemp -d)
trap 'rm -rf "$scratch"' EXIT

if ! jq -e '
  (.schema_version == 1)
  and (.acceptances | type == "array")
  and ([.acceptances[].base_sha] | length == (unique | length))
  and all(.acceptances[];
    (.base_sha | type == "string" and test("^[0-9a-f]{40}$"))
    and (.rationale | type == "string" and length > 0 and length <= 500)
    and (.entries | type == "array" and length > 0)
    and ([.entries[].path] | length == (unique | length))
    and all(.entries[];
      (.path | type == "string" and length > 0 and length <= 512)
      and (.path | startswith("/") | not)
      and (.path | split("/") | all(. != "" and . != "." and . != ".."))
      and (.path | test("[[:cntrl:]]") | not)
      and (.reason == "material_score_increase" or .reason == "worse_band" or .reason == "new_finding")
      and (.severity == "notice" or .severity == "warning")
      and (.content_sha256 | type == "string" and test("^[0-9a-f]{64}$"))
      and (.maximum_slop_score | type == "number" and . >= 0 and . <= 100)
    )
  )
' "$manifest" >/dev/null; then
  echo "dogfood regression acceptance manifest is invalid" >&2
  exit 1
fi

if ! jq -e --arg base "$base_sha" --arg head "$head_sha" '
  (.command == "compare")
  and (.schema_version == 1)
  and (.detail == "full")
  and (.policy_source == "base")
  and (.base_report.head_sha == $base)
  and (.head_report.head_sha == $head)
  and (.pagination.regressions.has_more == false)
  and (.summary.regression_count == (.regressions | length))
' "$comparison" >/dev/null; then
  echo "dogfood comparison is incomplete or incompatible" >&2
  exit 1
fi

if ! jq -e --arg head "$head_sha" '.repo.head_sha == $head' "$head_report" >/dev/null; then
  echo "dogfood head report does not match the exact pull-request head" >&2
  exit 1
fi

jq --slurpfile head "$head_report" '
  [.regressions[] as $regression
    | ($head[0].files | map(select(.path == $regression.path))) as $matches
    | if ($matches | length) != 1 then
        error("regression path is not uniquely present in the head report")
      else
        {
          path: $regression.path,
          reason: $regression.reason,
          severity: $regression.severity,
          content_sha256: $matches[0].content_sha256,
          slop_score: $regression.head_slop_score
        }
      end
  ] | sort_by(.path)
' "$comparison" >"$scratch/actual.json"

jq --arg base "$base_sha" '
  [.acceptances[] | select(.base_sha == $base)]
  | if length == 0 then {entries: []} else .[0] end
' "$manifest" >"$scratch/active.json"

if ! jq -e --slurpfile actual "$scratch/actual.json" '
  .entries as $accepted
  | $actual[0] as $observed
  | (($accepted | length) == ($observed | length))
    and all($observed[];
      . as $regression
      | any($accepted[];
          .path == $regression.path
          and .reason == $regression.reason
          and .severity == $regression.severity
          and .content_sha256 == $regression.content_sha256
          and .maximum_slop_score >= $regression.slop_score
        )
    )
' "$scratch/active.json" >/dev/null; then
  echo "dogfood regressions exceed or drift from the reviewed acceptance ledger" >&2
  exit 1
fi

regression_count=$(jq 'length' "$scratch/actual.json")
if [[ $regression_count -eq 0 ]]; then
  echo "Dogfood comparison has no regressions."
else
  echo "Dogfood comparison bound $regression_count reviewed regression(s) to exact content and score ceilings."
fi
