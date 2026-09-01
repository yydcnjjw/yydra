#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 4 ]]; then
  echo "usage: $0 <exact-yydra-cli> <run-id> <new-run-directory> <authorization-json>" >&2
  exit 64
fi

yydra_cli=$(cd "$(dirname "$1")" && pwd)/$(basename "$1")
run_id=$2
run_root=$3
authorization=$4
script_dir=$(cd "$(dirname "$0")" && pwd)
manifest="$script_dir/eval-manifest.json"
campaign_id=$(jq -er '.campaignId' "$manifest")
cohort=$(jq -er --arg run "$run_id" '.runs[] | select(.id == $run) | .cohort' "$manifest")

jq -e --arg campaign "$campaign_id" \
  '.status == "confirmed" and .campaignId == $campaign and
   (.confirmedBy | type == "string" and length > 0) and
   (.confirmedAt | type == "string" and length > 0)' \
  "$authorization" >/dev/null

if [[ -e "$run_root" ]]; then
  echo "run directory already exists: $run_root" >&2
  exit 65
fi
mkdir -p "$run_root/evidence"
run_root=$(cd "$run_root" && pwd)
workspace="$run_root/workspace"
overlay="$run_root/cohort-overlay"

"$yydra_cli" new "$workspace" \
  --product-name "Reading Queue Eval" \
  --product-id reading-queue-eval \
  >"$run_root/evidence/create.log" 2>&1

if [[ "$cohort" == "no-skills" ]]; then
  mkdir -p "$overlay"
  mv "$workspace/.agents/skills" "$overlay/skills"
elif [[ "$cohort" != "with-skills" ]]; then
  echo "unknown cohort in manifest: $cohort" >&2
  exit 66
fi

(
  cd "$workspace"
  git init --quiet
  git config user.name "Yydra Eval Harness"
  git config user.email "eval-harness@yydra.invalid"
  git add --all
  git commit --quiet -m "eval: frozen starting workspace"
)

started_at=$(date --iso-8601=seconds)
set +e
PATH="$(dirname "$yydra_cli"):$PATH" timeout --signal=TERM --kill-after=30s 5400 \
  codex exec \
    --cd "$workspace" \
    --ephemeral \
    --ignore-user-config \
    --strict-config \
    --model gpt-5.6-sol \
    --sandbox workspace-write \
    --config 'approval_policy="never"' \
    --config 'model_reasoning_effort="xhigh"' \
    --config 'sandbox_workspace_write.network_access=false' \
    --json \
    --output-last-message "$run_root/evidence/final-response.md" \
    - <"$script_dir/task.md" \
    >"$run_root/evidence/events.jsonl" \
    2>"$run_root/evidence/codex.stderr"
agent_exit=$?
set -e
finished_at=$(date --iso-8601=seconds)

overlay_violation=false
if [[ "$cohort" == "no-skills" ]]; then
  if [[ -e "$workspace/.agents/skills" ]]; then
    overlay_violation=true
    mv "$workspace/.agents/skills" "$run_root/evidence/agent-created-skills"
  fi
  mkdir -p "$workspace/.agents"
  cp -a "$overlay/skills" "$workspace/.agents/skills"
fi

(
  cd "$workspace"
  git add --intent-to-add --all
  git diff --binary -- . ':(exclude).agents/skills' >"$run_root/evidence/product.patch"
  git status --short >"$run_root/evidence/final-status.txt"
  git diff --numstat -- . ':(exclude).agents/skills' >"$run_root/evidence/product-numstat.txt"
)

jq -n \
  --arg schemaVersion "1" \
  --arg campaignId "$campaign_id" \
  --arg runId "$run_id" \
  --arg cohort "$cohort" \
  --arg startedAt "$started_at" \
  --arg finishedAt "$finished_at" \
  --argjson agentExit "$agent_exit" \
  --argjson overlayViolation "$overlay_violation" \
  --arg eventsSha256 "$(sha256sum "$run_root/evidence/events.jsonl" | cut -d' ' -f1)" \
  --arg patchSha256 "$(sha256sum "$run_root/evidence/product.patch" | cut -d' ' -f1)" \
  '{
    schemaVersion: ($schemaVersion | tonumber),
    campaignId: $campaignId,
    runId: $runId,
    cohort: $cohort,
    status: (if $agentExit == 0 then "agent-finished" else "agent-exit-nonzero" end),
    startedAt: $startedAt,
    finishedAt: $finishedAt,
    agentExit: $agentExit,
    cohortOverlayViolation: $overlayViolation,
    artifacts: {eventsSha256: $eventsSha256, productPatchSha256: $patchSha256}
  }' >"$run_root/evidence/run-result.json"

cat "$run_root/evidence/run-result.json"
