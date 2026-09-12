<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Simplify API generation

Status: accepted — 2026-09-08.

The maintainer confirmed the consolidated design, implementation scope, and
acceptance boundaries with “确认共同理解”. The design discussion is complete.

## Selected concurrency boundary

API generation will follow an ordinary sequential build workflow. The maintainer
does not require simultaneous generation against the same output directory and
considers the additional coordination logic unjustified for this use case.
Remove the dedicated API generation lock without introducing a replacement
concurrency mechanism. This decision concerns API generation coordination;
ordinary Cargo/npm dependency lockfiles retain their existing role.

The previous implementation acquired a shared or exclusive Workspace lock before
generation or checking. The selected boundary intentionally drops that
concurrent-invocation guarantee.

## Selected generation-entrypoint checks

API generation will validate the project structure, export entrypoint,
configuration, and tool compatibility that it actually depends on. Its
preconditions will not include a full Workspace Origin Record, exact
Distribution/template identity, creation fingerprint, license-file snapshot,
or distribution-inventory verification. Exact CLI release identity alone is
not a requirement when the inputs and tools satisfy the generation contract.

Keep full Workspace provenance and Distribution snapshot verification in
`doctor` and the relevant `check` nodes. The previous `generate_api` call to
`verify_workspace` combined those concerns; implementation must separate them
without weakening the dedicated checks or silently restoring the full gate
through another generation prerequisite. Workspace-root discovery may still
use the project's origin-file location without validating its complete contents.

This selects a narrower generation entrypoint than the current workflow under
[ADR 0001](0001-remove-supply-chain-from-current-workflow.md). Provenance checks
remain part of diagnosis and validation; this decision changes where they gate
execution. The maintainer selected this recommendation with “按推荐”.

## Selected build-artifact lifecycle

The exported Public API Contract and Generated Client will both be disposable,
rebuildable outputs in the build directory, rather than files committed to Git.
Rust handlers and their `utoipa` declarations remain the authored authority.
The maintainer selected this recommendation with “按推荐” in the artifact round.

Generate directly into the owned build-output location. Remove the separate
staging root, multi-output replacement transaction, rollback, and interrupted
transaction recovery. A failed generation fails the build and blocks downstream
consumers; the next generation can clean its owned outputs and rebuild them.
Retaining a previous successful generated output set is not a requirement.
Cleaning these outputs must not delete unrelated Cargo build caches.

Frontend compilation, tests, and development entrypoints must arrange generation
before consuming the Generated Client, including after a clean checkout or
build-directory cleanup. Rust contract checks must use the freshly derived
contract without requiring a committed JSON file before the exporter can build.
Update creation, packaging, import boundaries, and the check graph to reflect
this dependency order and output lifecycle. Comparing rebuilt files against a
committed generated copy is no longer an applicable check.

This replaces the previous committed-contract/client lifecycle. The template
[README](../../crates/yydra-cli/template/product-workspace/README.md) documents
the implemented build-output workflow.
Current-schema validation, generated-client validation, and type checking remain
part of generation; the build directory alone does not establish correctness.

## Selected current-version validation

Generation will validate the current Rust declarations, derived Public API
Contract, and Generated Client together. It will not compare the result with an
older API contract or require approval of a breaking change. The maintainer
selected this recommendation with “按推荐” after the older-client consequence
was explained.

Remove the persistent generation record, archived contracts and compatibility
history chain, predecessor/output digests used only by those records, breaking
change classification, and `--acknowledge-breaking-change`. Do not introduce a
replacement historical baseline or decision ledger. A successful build proves
current-version conformance; it makes no cross-version compatibility claim.
For example, a field deletion may pass after the current frontend is updated,
even though an older deployed client would be affected.

Keep OpenAPI profile checks, runtime request/response validation, generated-client
checks, TypeScript checking, and the relevant current-version conformance tests.
Artifact hashes used by the wider Conformance Evidence workflow retain their
existing purpose; removing API history is not a reason to remove those hashes.

## Implementation and acceptance boundary

The accepted implementation scope is:

- Use a dedicated API output subtree under Cargo's configured target directory,
  respecting target-directory configuration. Generate and validate in one
  sequential pipeline, and clean only that pipeline's owned outputs when a
  rebuild needs a clean directory.
- Remove the API-specific `--check` mode whose purpose was comparing committed
  outputs. The top-level `yydra check` command remains and uses the same current
  generation-and-validation pipeline. Update diagnostics and evidence claims
  to allow build outputs while preserving authored files and dependency locks.
- Update frontend imports and bundler/type-checker resolution, development and
  build prerequisites, Rust contract-test inputs, template packaging, generated
  path classification, agent guidance, and check-graph ordering together. A
  clean checkout must not need generated files before it can build the exporter.
- Apply the new workflow through a subsequent Distribution and its templates.
  Automatic migration or cleanup of existing Product Workspaces is outside this
  change. Existing projects must satisfy the new functional generation inputs
  to use the new entrypoint; a matching release label alone is insufficient.
- Verify a packaged CLI against a fresh Product Workspace: generate from a clean
  output directory, rebuild after cleanup, and consume the client in frontend
  type checking, tests, H5 and Android builds. Retain current API/runtime contract
  tests and rejection of invalid schemas, invalid generated clients, and
  incompatible generator configuration. Force a generation failure to verify
  downstream compilation stops and a later run can rebuild successfully.

Update tests of committed generated files, locking, transactions, history, and
exact identity rejection at the generation entrypoint to reflect the selected
contract. Dedicated `doctor`/`check` identity tests remain. Preserve historical
release records and unrelated working-tree changes.

Implemented in Distribution `0.3.0`. Direct generation follows Cargo's configured
target directory; `check` explicitly configures its isolated scratch `target/`
for every subprocess while preserving the original authored-input inventory.

Repository regressions passed (115 passed, 5 default ignores), and the packaged
CLI passed all 29 Clean Product checks, including real H5 and Android release
builds, after repairing a Gradle download timeout with a verified public wrapper
cache. Both review axes have no unresolved findings. See the
[implementation validation record](https://github.com/yydcnjjw/yydra/wiki/Validation-2026-09-08-API-Generation/6a878e624ef5fba3d3069e3daf38a7169d798982)
for package identities, failure/rebuild cases, evidence, and scope limits.

## Subsequent diagnostic and validation scope

[ADR 0009](0009-consolidate-diagnostics-in-doctor.md) retires `yydra check`,
the quality graph, and aggregate evidence. Current environment diagnostics use
`doctor`; project validation uses explicit Cargo/npm tests and builds. The
original decision and dated validation above retain their historical scope.
