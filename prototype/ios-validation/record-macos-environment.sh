#!/usr/bin/env bash
set -euo pipefail

workspace=${1:?usage: record-macos-environment.sh WORKSPACE OUTPUT_DIRECTORY}
output=${2:?usage: record-macos-environment.sh WORKSPACE OUTPUT_DIRECTORY}
frontend="$workspace/frontend"

mkdir -p "$output"
xcrun simctl list runtimes -j >"$output/simulator-runtimes.json"
xcrun simctl list devicetypes -j >"$output/simulator-device-types.json"
xcodebuild -showsdks >"$output/xcode-sdks.txt"

jq -n \
  --arg macos "$(sw_vers -productVersion)" \
  --arg macosBuild "$(sw_vers -buildVersion)" \
  --arg architecture "$(uname -m)" \
  --arg xcode "$(xcodebuild -version | paste -sd ';' -)" \
  --arg node "$(node --version)" \
  --arg npm "$(npm --version)" \
  --arg expo "$(cd "$frontend" && npm exec -- expo --version)" \
  --arg cocoapods "$(pod --version)" \
  --arg rustc "$(rustc --version)" \
  --arg cargo "$(cargo --version)" \
  --arg distribution "$(sed -n 's/^distribution_version = "\([^"]*\)"/\1/p' "$workspace/.yydra/origin.toml")" \
  --arg originSha256 "$(shasum -a 256 "$workspace/.yydra/origin.toml" | awk '{print $1}')" \
  --arg gitSha "${GITHUB_SHA:-$(git rev-parse HEAD)}" \
  --arg runnerImage "${ImageOS:-unknown}" \
  --arg runnerImageVersion "${ImageVersion:-unknown}" \
  '{
    schemaVersion: 1,
    macos: $macos,
    macosBuild: $macosBuild,
    architecture: $architecture,
    xcode: $xcode,
    node: $node,
    npm: $npm,
    expo: $expo,
    cocoapods: $cocoapods,
    rustc: $rustc,
    cargo: $cargo,
    distribution: $distribution,
    originSha256: $originSha256,
    gitSha: $gitSha,
    runnerImage: $runnerImage,
    runnerImageVersion: $runnerImageVersion
  }' >"$output/environment.json"
