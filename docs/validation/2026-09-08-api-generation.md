<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# API generation simplification validation

Date: 2026-09-08. Decision: [ADR 0002](../adr/0002-simplify-api-generation.md).
Candidate Distribution: `0.3.0`. Baseline commit:
`5d2a6478c46690ae9030b1adf2ce39e71802b989`.

Status: passed — repository regressions and packaged clean-Workspace acceptance.

## Behavior under test

Rust handlers and `utoipa` declarations remain the authored contract. API
outputs are rebuilt under Cargo's configured target directory, scoped by the
canonical Workspace path so sequential projects sharing a Cargo cache do not
overwrite each other. Generation exports and validates OpenAPI, runs the pinned
Orval generator, type-checks and validates its client, then links
`@yydra/generated-api` into frontend dependencies. Frontend entrypoints prepare
it before consuming it. Generation errors stop downstream commands; a subsequent
run cleans only its owned API output subtree and rebuilds.

No API generation lock, staging transaction, recovery journal, compatibility
ledger, breaking-change acknowledgement, or API-specific `--check` remains.
Generation verifies its functional inputs and generator version. Complete
provenance/snapshot checks remain in `doctor` and dedicated check nodes.

The check graph sets `CARGO_TARGET_DIR` to its isolated scratch Workspace's
`target/` for every subprocess, including the generated-client hooks and runtime
server. This leaves original authored-input scanning intact even when a project
configures `target-dir = "."`. Direct generation respects the caller's Cargo
configuration. Existing custom build directories in the original project are
conservatively included in the input inventory; this change does not introduce a
broader dynamic input exclusion policy.

## Candidate identity

| Artifact | Bytes | SHA-256 |
| --- | ---: | --- |
| `yydra-cli-0.3.0.crate` | 331632 | `eda705ea178bb7f700c26cb6c8439d37fa327cfa8e980085dbd5b30356d13c34` |
| Installed `yydra` | 4409112 | `ab75ca9310b00b3c47ecc9f59bf7d94b50efde912701c1596441d3b4454d088c` |

All 3 packaged Rust source files and all 84 packaged template files were compared
byte-for-byte with the implementation in the working tree.

## Regression and consumer evidence

Local evidence root:
`/home/yydcnjjw/.cache/yydra-api-generation-20260908/`.

Repository validation passed:

- `cargo fmt --all --check`.
- `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings`.
- `CARGO_BUILD_JOBS=2 TMPDIR=<evidence-root>/tmp cargo test --locked --workspace
  --all-targets --all-features -- --test-threads=1`: **115 passed, 0 failed,
  5 ignored**, across 8 suites (`full-suite.log`). The ignored tests retain their
  existing environment prerequisites; live packaged-consumer acceptance runs
  separately below.
- After registering the final executable-discovery diagnostic, repeated the
  formatter/Clippy checks and the 12 CLI unit tests (`final-static.log`); all passed.

The packaged CLI also rejects both removed flags with exit code 2
(`candidate/removed-flags.json`).

- Fast CLI regressions cover disposable output creation, narrower generation
  prerequisites with retained doctor rejection, clean rebuilds, partial-failure
  recovery, unrelated-cache preservation, sequential shared-cache consumers,
  current-schema/client/tool-version rejection, and frontend failure propagation.
- A real check-node regression rejects an invalid generated header while retaining
  authored inputs. Runtime contract tests derive the current OpenAPI directly;
  exporter compilation no longer needs a committed JSON artifact.
- The custom Cargo target regression failed against the previous implementation
  with `target-dir = "build"` (`custom-target-red.log`, 112.67 seconds). Its fixed
  version covers both `build` and `.` with unchanged original inputs.
- Development-workspace checks used real Rust, pinned Orval/TypeScript, Vitest,
  and Metro. Type checking, all 45 frontend tests, formatting, lint, and production
  H5 export passed before packaged-consumer validation.

The packaged acceptance controller and its machine-readable receipt are retained
in `acceptance-candidate.py` and `candidate/receipt.json`. It verifies package
bytes, installs the extracted crate, compares emitted Baseline Skill bytes with
the packaged templates, creates a fresh Product Workspace, runs doctor/setup,
rebuilds after removing only the API output subtree, consumes the client in
frontend checks/H5, forces a real generator failure and repairs it, and executes
the complete `yydra check` graph including Android release.

Cargo package archives normalize source mtimes. Reusing the Cargo target cache
for another install of the same candidate version initially reused an older
executor. The final controller refreshes only the extracted entrypoint's mtime
before compilation and verifies emitted template bytes; source bytes and cached
dependencies are preserved. Earlier interrupted receipts are not final evidence.

## Infrastructure retry

The initial complete graph passed 28 of 29 nodes, including real H5 semantics
and the original-input immutability check. Android alone reported
`CHECK_TOOL_VERSION_UNAVAILABLE`: downloading `gradle-9.3.1-bin.zip` hit the
wrapper's 10-second socket read timeout. The failure remains in
`candidate/clean-evidence/manifest.json` and `candidate/receipt.json`.

`retry-conformance.py` reruns the same packaged executor and complete graph in a
new evidence root. It seeds only the installed public Gradle 9.3.1 wrapper cache
before native execution, omits its lock file, and verifies each copied file
against the source. The inventory has 315 files,
151910344 bytes, and SHA-256
`95a3c5057549b6661538e385cc2fbbc2c7dbc9159aa6726adbef785904f00e56`.
It is retained as `candidate/wrapper-cache-inventory.json`. Authored inputs,
generated native source, tool versions, build commands, and check requirements
are unchanged. The retry result is retained in `candidate/retry-receipt.json` and
`candidate/clean-evidence-retry/manifest.json`.

## Final packaged result

The same packaged executor passed the complete Clean Product graph on the retry:
**29/29 nodes passed**, `complete: true`, `status: pass-core`. This includes current
API/runtime conformance, frontend type checking/tests, live PostgreSQL/H5 behavior,
accessibility semantics, two reproducible native generations, a real Android
release build, and unchanged original Workspace inputs. Android Metro bundled
1,420 modules from the new client layout. The retained APK has
**96,927,296 bytes** and SHA-256
`1ad8a93f1115acc4100ab62d01ae597ab2b974a95b82ac617a1bb12de8500604`.

The final manifest SHA-256 is
`6ad9a55c664a9022851fdf02777b102a5b971ac202ec7878816112f3417251d2`.
`candidate/final-receipt.json` combines package/executor identity, successful
consumer checks, the initial infrastructure failure, and the passing retry.
The final authored-file inventory was also compared with creation and remained
identical. The previous failed manifest remains an unsuccessful attempt.

## Review

Standards: no documented-standard violations or actionable code smells found.

Spec: two findings were corrected and re-reviewed: custom Cargo output settings
now remain isolated within check, and obsolete acknowledgement/busy guidance was
removed. No unresolved implementation findings remain. The executable acceptance
results above also passed.

## Limits

This validates current-version agreement. It does not promise compatibility with
older clients, migrate existing Product Workspaces, support concurrent generation
against the same owned output, or establish Android device/runtime/accessibility
or iOS behavior. A full local check is not aggregate conformance across fixtures.
