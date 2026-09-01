#!/usr/bin/env bash
set -euo pipefail

workspace=${1:?usage: capture-ios-cng.sh WORKSPACE OUTPUT_DIRECTORY}
output=${2:?usage: capture-ios-cng.sh WORKSPACE OUTPUT_DIRECTORY}
frontend="$workspace/frontend"
native="$frontend/ios"

[[ $(uname -s) == Darwin ]] || {
  echo "iOS CNG evidence requires macOS" >&2
  exit 1
}
[[ -f "$frontend/package-lock.json" ]] || {
  echo "missing frontend package lock: $frontend/package-lock.json" >&2
  exit 1
}
[[ ! -e "$native" ]] || {
  echo "generated iOS host must be absent before evidence capture: $native" >&2
  exit 1
}
[[ ! -e "$output" ]] || {
  echo "evidence directory already exists: $output" >&2
  exit 1
}

mkdir -p "$output"

remove_native() {
  if [[ -d "$native" ]]; then
    chmod -R u+w "$native" 2>/dev/null || true
    rm -rf -- "$native"
  fi
}
trap remove_native EXIT

capture_inventory() {
  local run=$1
  (
    cd "$native"
    find . -type f -print | LC_ALL=C sort | while IFS= read -r relative; do
      digest=$(shasum -a 256 "$relative" | awk '{print $1}')
      printf '%s  %s\n' "$digest" "${relative#./}"
    done
  ) >"$output/$run.sha256"
  (
    cd "$native"
    find . -type f -print | LC_ALL=C sort | while IFS= read -r relative; do
      mode_and_size=$(stat -f '%Lp %z' "$relative")
      printf '%s %s\n' "$mode_and_size" "${relative#./}"
    done
  ) >"$output/$run.modes"
}

git -C "$workspace" status --porcelain=v1 --untracked-files=all -- . \
  >"$output/authored-before.status"

for run in run-1 run-2; do
  (
    cd "$frontend"
    CI=1 npm exec -- expo prebuild --platform ios --clean --no-install
  ) 2>&1 | tee "$output/$run.log"
  capture_inventory "$run"
  remove_native
done

cmp "$output/run-1.sha256" "$output/run-2.sha256"
cmp "$output/run-1.modes" "$output/run-2.modes"

git -C "$workspace" status --porcelain=v1 --untracked-files=all -- . \
  >"$output/authored-after.status"
cmp "$output/authored-before.status" "$output/authored-after.status"

jq -n \
  --arg generator "$(cd "$frontend" && npm exec -- expo --version)" \
  --arg inventorySha256 "$(shasum -a 256 "$output/run-1.sha256" | awk '{print $1}')" \
  --arg modesSha256 "$(shasum -a 256 "$output/run-1.modes" | awk '{print $1}')" \
  --argjson fileCount "$(wc -l <"$output/run-1.sha256" | tr -d ' ')" \
  '{
    schemaVersion: 1,
    status: "pass",
    generator: $generator,
    runs: 2,
    fileCount: $fileCount,
    pathAndByteInventorySha256: $inventorySha256,
    pathSizeAndModeInventorySha256: $modesSha256,
    authoredSourceUnchanged: true
  }' >"$output/result.json"
