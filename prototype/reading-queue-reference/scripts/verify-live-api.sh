#!/usr/bin/env bash
set -euo pipefail

api_base=${API_BASE_URL:-http://127.0.0.1:4000}
output=${1:-evidence/live-api-smoke.json}
probe_tmp=$(mktemp -d)
trap 'rm -rf -- "$probe_tmp"' EXIT

request() {
  local name=$1
  shift
  curl -sS -D "$probe_tmp/$name.headers" -o "$probe_tmp/$name.body" \
    -w '%{http_code}' "$@"
}

expect_status() {
  local actual=$1
  local expected=$2
  local label=$3
  if [[ "$actual" != "$expected" ]]; then
    echo "$label: expected HTTP $expected, got $actual" >&2
    cat "$probe_tmp/$label.body" >&2 2>/dev/null || true
    exit 1
  fi
}

health_status=$(request health "$api_base/health")
expect_status "$health_status" 200 health
jq -e '.status == "ready"' "$probe_tmp/health.body" >/dev/null

unique=$(date +%s%N)
create_status=$(request create -X POST "$api_base/reading-entries" \
  -H 'content-type: application/json' \
  --data "{\"title\":\"Live API $unique\",\"sourceUrl\":\"https://example.com/live-api/$unique\"}")
expect_status "$create_status" 201 create
entry_id=$(jq -er '.id' "$probe_tmp/create.body")
jq -e '.status == "queued"' "$probe_tmp/create.body" >/dev/null

complete_status=$(request complete -X POST "$api_base/reading-entries/$entry_id/complete")
expect_status "$complete_status" 200 complete
jq -e '.status == "completed"' "$probe_tmp/complete.body" >/dev/null

conflict_status=$(request conflict -X POST "$api_base/reading-entries/$entry_id/complete")
expect_status "$conflict_status" 409 conflict
grep -Eiq '^content-type: application/problem\+json' "$probe_tmp/conflict.headers"
jq -e '
  .type == "https://yydra.dev/problems/invalid-reading-entry-transition" and
  .status == 409 and
  (.traceId | type == "string" and length > 0)
' "$probe_tmp/conflict.body" >/dev/null

filtered_status=$(request filtered "$api_base/reading-entries?status=completed&limit=50")
expect_status "$filtered_status" 200 filtered
jq -e --arg id "$entry_id" '.items | any(.id == $id and .status == "completed")' \
  "$probe_tmp/filtered.body" >/dev/null

reopen_status=$(request reopen -X POST "$api_base/reading-entries/$entry_id/reopen")
expect_status "$reopen_status" 200 reopen
jq -e '.status == "queued"' "$probe_tmp/reopen.body" >/dev/null

invalid_input_status=$(request invalid-input -X POST "$api_base/reading-entries" \
  -H 'content-type: application/json' \
  --data '{"title":"","sourceUrl":"not-a-url"}')
expect_status "$invalid_input_status" 400 invalid-input
jq -e '.type == "https://yydra.dev/problems/invalid-input" and .status == 400' \
  "$probe_tmp/invalid-input.body" >/dev/null

invalid_cursor_status=$(request invalid-cursor "$api_base/reading-entries?cursor=not-opaque")
expect_status "$invalid_cursor_status" 400 invalid-cursor
jq -e '.type == "https://yydra.dev/problems/invalid-cursor" and .status == 400' \
  "$probe_tmp/invalid-cursor.body" >/dev/null

page_one_status=$(request page-one "$api_base/reading-entries?limit=1")
expect_status "$page_one_status" 200 page-one
cursor=$(jq -er '.nextCursor' "$probe_tmp/page-one.body")
encoded_cursor=$(jq -rn --arg value "$cursor" '$value|@uri')
page_two_status=$(request page-two "$api_base/reading-entries?limit=1&cursor=$encoded_cursor")
expect_status "$page_two_status" 200 page-two
page_one_id=$(jq -er '.items[0].id' "$probe_tmp/page-one.body")
page_two_id=$(jq -er '.items[0].id' "$probe_tmp/page-two.body")
[[ "$page_one_id" != "$page_two_id" ]]

cors_status=$(request cors -X OPTIONS "$api_base/reading-entries" \
  -H 'origin: http://127.0.0.1:8081' \
  -H 'access-control-request-method: POST')
[[ "$cors_status" == 200 || "$cors_status" == 204 ]]
grep -Eiq '^access-control-allow-origin:' "$probe_tmp/cors.headers"

jq -n \
  --arg apiBase "$api_base" \
  --arg entryId "$entry_id" \
  --arg pageOneId "$page_one_id" \
  --arg pageTwoId "$page_two_id" \
  '{
    schemaVersion: 1,
    status: "pass",
    apiBase: $apiBase,
    entryId: $entryId,
    checks: [
      "health-ready",
      "create-queued",
      "complete",
      "repeat-complete-stable-problem",
      "completed-filter",
      "reopen",
      "invalid-input-stable-problem",
      "invalid-cursor-stable-problem",
      "opaque-cursor-no-adjacent-duplicate",
      "cors-preflight"
    ],
    paginationEvidence: {
      pageOneId: $pageOneId,
      pageTwoId: $pageTwoId
    }
  }' >"$output"

cat "$output"
