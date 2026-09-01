#!/usr/bin/env bash
set -euo pipefail

api_base=${API_BASE_URL:-http://127.0.0.1:4000}
output=${1:-evidence/live-api-smoke.json}
probe_tmp=$(mktemp -d)
trap 'rm -rf -- "$probe_tmp"' EXIT

curl -fsS "$api_base/health" >"$probe_tmp/health.json"
jq -e '.status == "ready"' "$probe_tmp/health.json" >/dev/null
mkdir -p "$(dirname "$output")"
jq -n \
  --arg apiBase "$api_base" \
  '{
    schemaVersion: 1,
    status: "pass",
    apiBase: $apiBase,
    checks: ["health-ready"]
  }' >"$output"

cat "$output"
