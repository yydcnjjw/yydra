#!/usr/bin/env bash
set -euo pipefail

workspace=${1:?usage: run-ios-runtime.sh WORKSPACE APP_BUNDLE EVIDENCE_DIRECTORY}
application=${2:?usage: run-ios-runtime.sh WORKSPACE APP_BUNDLE EVIDENCE_DIRECTORY}
evidence=${3:?usage: run-ios-runtime.sh WORKSPACE APP_BUNDLE EVIDENCE_DIRECTORY}
flow="$(cd "$(dirname "$0")" && pwd -P)/reading-queue.yaml"

[[ $(uname -s) == Darwin ]] || {
  echo "iOS runtime evidence requires macOS" >&2
  exit 1
}
[[ -d "$application" && "$application" == *.app ]] || {
  echo "missing iOS simulator application bundle: $application" >&2
  exit 1
}
[[ ! -e "$evidence/result.json" ]] || {
  echo "completed evidence already exists: $evidence/result.json" >&2
  exit 1
}

mkdir -p "$evidence/maestro-output" "$evidence/maestro-debug"

postgres_bin="$(brew --prefix postgresql@18)/bin"
export PATH="$postgres_bin:$PATH"
test "$(postgres --version)" = "postgres (PostgreSQL) 18.6 (Homebrew)"
maestro --version | tee "$evidence/maestro-version.txt"
grep -Eq '(^|[^0-9])2\.7\.0([^0-9]|$)' "$evidence/maestro-version.txt"

database_directory="${RUNNER_TEMP:?}/yydra-ios-postgres"
database_log="$evidence/postgres.log"
server_log="$evidence/backend.log"
simulator_log="$evidence/simulator.log"
server_pid=
simulator_log_pid=
simulator_udid=
database_started=false

cleanup() {
  local status=$?
  if [[ -n "$simulator_log_pid" ]]; then
    kill "$simulator_log_pid" 2>/dev/null || true
    wait "$simulator_log_pid" 2>/dev/null || true
  fi
  if [[ -n "$server_pid" ]]; then
    kill "$server_pid" 2>/dev/null || true
    wait "$server_pid" 2>/dev/null || true
  fi
  if [[ "$database_started" == true ]]; then
    pg_ctl -D "$database_directory" -m fast -w stop >/dev/null 2>&1 || true
  fi
  if [[ -n "$simulator_udid" ]]; then
    xcrun simctl shutdown "$simulator_udid" >/dev/null 2>&1 || true
    xcrun simctl delete "$simulator_udid" >/dev/null 2>&1 || true
  fi
  trap - EXIT
  exit "$status"
}
trap cleanup EXIT

[[ ! -e "$database_directory" ]] || {
  echo "PostgreSQL scratch directory already exists: $database_directory" >&2
  exit 1
}
initdb -D "$database_directory" --username=postgres --auth=trust --no-locale --encoding=UTF8 \
  >"$evidence/initdb.log"
pg_ctl -D "$database_directory" -l "$database_log" \
  -o "-h 127.0.0.1 -p 5432" -w start
database_started=true
createdb -h 127.0.0.1 -p 5432 -U postgres yydra_reading_queue

database_url=postgres://postgres@127.0.0.1:5432/yydra_reading_queue
(
  cd "$workspace"
  DATABASE_URL="$database_url" cargo run --locked --bin migrate
) >"$evidence/migrate.log" 2>&1
(
  cd "$workspace"
  DATABASE_URL="$database_url" \
    YYDRA_BIND_ADDRESS=127.0.0.1:4000 \
    RUST_LOG=info \
    cargo run --locked --bin server
) >"$server_log" 2>&1 &
server_pid=$!

for _ in $(seq 1 120); do
  if curl -fsS http://127.0.0.1:4000/health >"$evidence/health.json" 2>/dev/null; then
    break
  fi
  if ! kill -0 "$server_pid" 2>/dev/null; then
    echo "Reading Queue backend exited before readiness" >&2
    exit 1
  fi
  sleep 1
done
jq -e '.status == "ready"' "$evidence/health.json" >/dev/null

runtime_identifier=com.apple.CoreSimulator.SimRuntime.iOS-26-5
device_type=com.apple.CoreSimulator.SimDeviceType.iPhone-17
xcrun simctl list runtimes -j >"$evidence/simulator-runtimes.json"
xcrun simctl list devicetypes -j >"$evidence/simulator-device-types.json"
jq -e --arg id "$runtime_identifier" \
  '.runtimes[] | select(.identifier == $id and .isAvailable == true)' \
  "$evidence/simulator-runtimes.json" >/dev/null
jq -e --arg id "$device_type" \
  '.devicetypes[] | select(.identifier == $id)' \
  "$evidence/simulator-device-types.json" >/dev/null

simulator_udid=$(xcrun simctl create "Yydra V0 iOS validation" "$device_type" "$runtime_identifier")
xcrun simctl boot "$simulator_udid"
xcrun simctl bootstatus "$simulator_udid" -b
xcrun simctl install "$simulator_udid" "$application"
xcrun simctl spawn "$simulator_udid" log stream --style compact --level info \
  --predicate 'process == "ReadingQueueTestbed"' >"$simulator_log" 2>&1 &
simulator_log_pid=$!

plist="$application/Info.plist"
test "$(plutil -extract NSAppTransportSecurity.NSAllowsLocalNetworking raw "$plist")" = true

MAESTRO_CLI_NO_ANALYTICS=1 maestro --device "$simulator_udid" test \
  --format junit \
  --output "$evidence/maestro-junit.xml" \
  --test-output-dir "$evidence/maestro-output" \
  --debug-output "$evidence/maestro-debug" \
  "$flow" | tee "$evidence/maestro.log"

xcrun simctl io "$simulator_udid" screenshot "$evidence/reading-queue-final.png"
MAESTRO_CLI_NO_ANALYTICS=1 maestro --device "$simulator_udid" hierarchy \
  >"$evidence/final-hierarchy.json"

psql -h 127.0.0.1 -p 5432 -U postgres -d yydra_reading_queue -At -F $'\t' \
  -c "SELECT title, source_url, status FROM reading_entries WHERE title = 'Issue 20 iOS runtime'" \
  >"$evidence/database-row.tsv"
grep -Fx $'Issue 20 iOS runtime\thttps://example.com/issue-20-ios-runtime\tqueued' \
  "$evidence/database-row.tsv"

(
  cd "$application"
  find . -type f -print | LC_ALL=C sort | while IFS= read -r relative; do
    digest=$(shasum -a 256 "$relative" | awk '{print $1}')
    printf '%s  %s\n' "$digest" "${relative#./}"
  done
) >"$evidence/application.sha256"

jq -n \
  --arg bundleIdentifier "$(plutil -extract CFBundleIdentifier raw "$plist")" \
  --arg applicationSha256 "$(shasum -a 256 "$evidence/application.sha256" | awk '{print $1}')" \
  --arg macos "$(sw_vers -productVersion)" \
  --arg macosBuild "$(sw_vers -buildVersion)" \
  --arg architecture "$(uname -m)" \
  --arg xcode "$(xcodebuild -version | paste -sd ';' -)" \
  --arg simulatorRuntime "$runtime_identifier" \
  --arg simulatorDeviceType "$device_type" \
  --arg simulatorUdid "$simulator_udid" \
  --arg node "$(node --version)" \
  --arg npm "$(npm --version)" \
  --arg expo "$(cd "$workspace/frontend" && npm exec -- expo --version)" \
  --arg cocoapods "$(pod --version)" \
  --arg postgres "$(postgres --version)" \
  --arg maestro "$(tr -d '\n' <"$evidence/maestro-version.txt")" \
  --arg distribution "$(sed -n 's/^distribution_version = "\([^"]*\)"/\1/p' "$workspace/.yydra/origin.toml")" \
  --arg originSha256 "$(shasum -a 256 "$workspace/.yydra/origin.toml" | awk '{print $1}')" \
  '{
    schemaVersion: 1,
    status: "pass",
    bundleIdentifier: $bundleIdentifier,
    applicationSha256: $applicationSha256,
    environment: {
      macos: $macos,
      macosBuild: $macosBuild,
      architecture: $architecture,
      xcode: $xcode,
      simulatorRuntime: $simulatorRuntime,
      simulatorDeviceType: $simulatorDeviceType,
      simulatorUdid: $simulatorUdid,
      node: $node,
      npm: $npm,
      expo: $expo,
      cocoapods: $cocoapods,
      postgres: $postgres,
      maestro: $maestro
    },
    distribution: $distribution,
    originSha256: $originSha256,
    coldLaunch: "pass-on-fresh-simulator",
    lifecycle: ["list-empty", "create-queued", "complete", "reopen-queued"],
    realBackendRowVerified: true,
    physicalDevice: "not-run"
  }' >"$evidence/result.json"
