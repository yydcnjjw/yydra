#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 4 ]]; then
  echo "usage: $0 <yydra-cli> <clean-workspace> <known-good-reference> <evidence-dir>" >&2
  exit 64
fi

yydra_cli=$1
clean_workspace=$2
known_good=$3
evidence_dir=$4
script_dir=$(cd "$(dirname "$0")" && pwd)

mkdir -p "$evidence_dir"

run_check() {
  local name=$1
  local workspace=$2
  local log="$evidence_dir/$name-check.log"
  local exit_file="$evidence_dir/$name-check.exit"

  set +e
  "$yydra_cli" check "$workspace" >"$log" 2>&1
  local check_exit=$?
  set -e
  printf '%s\n' "$check_exit" >"$exit_file"
}

"$yydra_cli" doctor "$clean_workspace" >"$evidence_dir/clean-doctor.log" 2>&1
"$yydra_cli" doctor "$known_good" >"$evidence_dir/reference-doctor.log" 2>&1
run_check clean "$clean_workspace"
run_check reference "$known_good"

reasons=()
for cohort in clean reference; do
  check_exit=$(<"$evidence_dir/$cohort-check.exit")
  if [[ "$check_exit" -ne 0 ]]; then
    reasons+=("${cohort^^}_CHECK_FAILED")
  fi
  if ! grep -qx 'status=pass' "$evidence_dir/$cohort-check.log"; then
    reasons+=("${cohort^^}_AGGREGATE_CHECK_INCOMPLETE")
  fi
done

required_nodes=(
  database.running-service
  h5.e2e
  android.release
  ios.simulator-release
)
for node in "${required_nodes[@]}"; do
  if grep -q "not-run=.*$node" "$evidence_dir/reference-check.log"; then
    reasons+=("REFERENCE_REQUIRED_NODE_NOT_RUN:$node")
  fi
done

if ! command -v xcodebuild >/dev/null 2>&1; then
  reasons+=("GRADER_IOS_TOOLCHAIN_UNAVAILABLE")
fi
if [[ ! -x "$script_dir/grader/run-hidden" ]]; then
  reasons+=("TASK_ACCEPTANCE_GRADER_UNAVAILABLE")
fi

status=ready
if [[ ${#reasons[@]} -gt 0 ]]; then
  status=campaign-invalid
fi

printf '%s\n' "${reasons[@]}" | jq -R . | jq -s \
  --arg status "$status" \
  --arg cli_sha256 "$(sha256sum "$yydra_cli" | cut -d' ' -f1)" \
  --arg clean_check_sha256 "$(sha256sum "$evidence_dir/clean-check.log" | cut -d' ' -f1)" \
  --arg reference_check_sha256 "$(sha256sum "$evidence_dir/reference-check.log" | cut -d' ' -f1)" \
  '{
    schemaVersion: 1,
    status: $status,
    reasons: .,
    artifacts: {
      cliSha256: $cli_sha256,
      cleanCheckLogSha256: $clean_check_sha256,
      referenceCheckLogSha256: $reference_check_sha256
    }
  }' >"$evidence_dir/preflight-result.json"

cat "$evidence_dir/preflight-result.json"
[[ "$status" == ready ]]
