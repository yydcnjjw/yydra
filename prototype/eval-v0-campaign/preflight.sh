#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 3 ]]; then
  echo "usage: $0 <exact-yydra-cli> <known-good-reference> <new-evidence-directory>" >&2
  exit 64
fi

yydra_cli=$(cd "$(dirname "$1")" && pwd)/$(basename "$1")
reference=$(cd "$2" && pwd)
evidence=$3
script_dir=$(cd "$(dirname "$0")" && pwd)
hash_tree="$script_dir/hash-tree"

if [[ -e "$evidence" ]]; then
  echo "evidence path already exists: $evidence" >&2
  exit 65
fi
mkdir -p "$evidence"
evidence=$(cd "$evidence" && pwd)
clean="$evidence/clean-workspace"

"$yydra_cli" new "$clean" \
  --product-name "Reading Queue Eval" \
  --product-id reading-queue-eval \
  >"$evidence/clean-create.log" 2>&1
"$yydra_cli" doctor "$clean" >"$evidence/clean-doctor.log" 2>&1
"$yydra_cli" doctor "$reference" >"$evidence/reference-doctor.log" 2>&1

set +e
"$script_dir/grader/run-hidden" "$clean" "$evidence/clean-hidden" \
  >"$evidence/clean-hidden.stdout" 2>"$evidence/clean-hidden.stderr"
clean_hidden_exit=$?
set -e
if [[ "$clean_hidden_exit" -eq 0 ]]; then
  echo "clean Workspace unexpectedly passed hidden task acceptance" >&2
  exit 1
fi

"$script_dir/grader/run-hidden" "$reference" "$evidence/reference-hidden" \
  >"$evidence/reference-hidden.stdout" 2>"$evidence/reference-hidden.stderr"

jq -n \
  --arg cliSha256 "$(sha256sum "$yydra_cli" | cut -d' ' -f1)" \
  --arg taskSha256 "$(sha256sum "$script_dir/task.md" | cut -d' ' -f1)" \
  --arg rubricSha256 "$(sha256sum "$script_dir/rubric.md" | cut -d' ' -f1)" \
  --arg graderSha256 "$("$hash_tree" "$script_dir/grader")" \
  --arg cleanInventorySha256 "$("$hash_tree" "$clean")" \
  --arg referenceInventorySha256 "$("$hash_tree" "$reference")" \
  --arg baselineSkillInventorySha256 "$("$hash_tree" "$clean/.agents/skills")" \
  --argjson cleanHiddenExit "$clean_hidden_exit" \
  '{
    schemaVersion: 1,
    status: "pass-local-discrimination",
    aggregateReady: false,
    cleanHiddenExit: $cleanHiddenExit,
    referenceHidden: "pass",
    artifacts: {
      cliSha256: $cliSha256,
      taskSha256: $taskSha256,
      rubricSha256: $rubricSha256,
      graderInventorySha256: $graderSha256,
      cleanWorkspaceInventorySha256: $cleanInventorySha256,
      knownGoodInventorySha256: $referenceInventorySha256,
      baselineSkillInventorySha256: $baselineSkillInventorySha256
    },
    remainingGate: "clean/reference full aggregate preflight on Linux plus macOS"
  }' >"$evidence/result.json"

cat "$evidence/result.json"
