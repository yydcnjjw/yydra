<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Safe repair routes

Route by the stable `cause.code`, then follow the remediation embedded in the
same exact Distribution catalog. Use only an exact code or explicitly enumerated
set below; headings and shared prefixes are not default routes. If a code is not
listed, retain the result and raw log, follow its catalog remediation, and stop
rather than guessing which authority or external condition to change.

## Doctor

- `DOCTOR_WORKSPACE_VERIFY` is the stable verification envelope. Follow its
  structured message and remediation: install the exact CLI named by a valid
  Origin Record for a Distribution mismatch; restore missing or drifted Origin,
  inventory, provenance, license, or Baseline Skill snapshot bytes from reviewed
  version control; and recreate from the exact packaged CLI if the origin schema,
  template identity, or template digest is not authentic. Do not edit those
  authorities into agreement. If the message does not identify one of these
  states, stop and retain it instead of guessing a repair.

## Origin and ownership

- `ORIGIN_AUTHORITY_DRIFT`, `FIXTURE_IDENTITY_MISMATCH`, and
  `BASELINE_SKILL_INVENTORY_DRIFT`: verify the exact CLI version and restore
  reviewed Distribution snapshot bytes. Do not reconstruct or modernize them by
  hand.
- `GENERATED_SNAPSHOT_DRIFT`: restore reviewed committed authority bytes or use
  the owning supported generator. Do not edit the generated file directly.
- `CHECK_MUTATED_ORIGINAL_INPUTS` and `CHECK_MUTATED_WORKSPACE_INPUTS`: stop the
  tool path that wrote authored or protected inputs; compare the before and after
  inventory before retrying.

## Rust and database

- `RUST_TOOLCHAIN_AUTHORITY_DRIFT`: restore the exact Distribution-owned
  `rust-toolchain.toml`; do not select a different toolchain to bypass a failure.
- `CARGO_LOCK_DRIFT`: repair the Product-owned manifest choice when it is wrong,
  then deliberately update and review the committed lock. Do not let a check or
  metadata probe rewrite it.
- `RUST_FORMAT_FAILED`: format the reported Rust source with the pinned toolchain.
- `RUST_COMPILE_FAILED`, `RUST_CLIPPY_FAILED`, `RUST_TEST_FAILED`, and
  `RUST_DOCTEST_FAILED`: inspect the exact retained Cargo output and repair its
  Product-owned source or test owner.
- `RUST_TEST_DISCOVERY_FAILED` and `RUST_TESTS_EMPTY`: restore discoverable
  canonical tests; do not treat zero tests as success.
- `ARCH_DEPENDENCY_CYCLE`, `ARCH_FORBIDDEN_DEPENDENCY`,
  `ARCH_FORBIDDEN_LAYER_EDGE`, `ARCH_FRAMEWORK_INTERNAL_DEPENDENCY`,
  `ARCH_METADATA_INVALID`, `ARCH_UNKNOWN_WORKSPACE_ROLE`, and
  `ARCH_WORKSPACE_PATH_ESCAPE`: repair the reported manifest, role, path, or
  dependency edge without weakening the architecture graph.
- `DB_MIGRATION_COMPARISON_BASE_INVALID`,
  `DB_MIGRATION_COMPARISON_BASE_UNAVAILABLE`, and
  `DB_MIGRATION_COMPARISON_FAILED`: correct or restore the requested Git revision
  or Git operation. Do not edit migrations to repair comparison infrastructure.
- `DB_MIGRATION_DISTRIBUTION_BASE_DELETED`,
  `DB_MIGRATION_DISTRIBUTION_BASE_MUTATED`,
  `DB_MIGRATION_COMPARISON_BASE_DELETED`, and
  `DB_MIGRATION_COMPARISON_BASE_MUTATED`: restore every reported prior migration
  byte, then add a new forward migration for an intended correction.
- `DB_MIGRATION_AUTHORITY_AMBIGUOUS`, `DB_MIGRATION_HISTORY_INVALID`, and
  `DB_MIGRATION_HISTORY_MISSING`: restore one valid root migration authority and
  sequential history before adding new work.
- `DATABASE_POSTGRES_UNAVAILABLE` and `DATABASE_POSTGRES_CLEANUP_FAILED`: repair
  or obtain the exact PostgreSQL service and cleanup condition; do not edit a
  domain invariant to disguise infrastructure.
- `DATABASE_MIGRATION_FAILED`: inspect the retained migration log and repair only
  the migration or database condition it identifies.
- `DATABASE_RUNTIME_INVARIANTS_FAILED`,
  `DATABASE_RUNTIME_INVARIANT_TEST_MISSING`, `READING_QUEUE_POSTGRES_FAILED`, and
  `READING_QUEUE_PAGINATION_POSTGRES_FAILED`: inspect the use-case transaction,
  concrete persistence, database constraints, and canonical test together; do
  not add a semantic retry.

## Public API and generation

- `API_GENERATION_BUSY`: another invocation owns the Workspace generation lock.
  Do not edit API source or remove a live lock. Let the owning invocation finish
  or stop the duplicate invocation, verify that ownership has ended, then rerun
  once because the coordination state changed.
- `API_GENERATION_RECOVERY_REQUIRED` and `API_GENERATION_RECOVERY_FAILED`: first
  verify that no live invocation owns the lock, then follow the generator's
  recovery remediation. Preserve the journal and previous complete outputs for
  diagnosis; do not reconstruct committed generated files by hand.
- `API_GENERATION_LOCK_INVALID`, `API_GENERATION_BASELINE_INVALID`, and
  `API_GENERATION_RECORD_DRIFT`: restore the committed lock or complete reviewed
  generation authority chain from version control. Do not delete the lock or
  edit records, history, OpenAPI, or client digests into agreement.
- `API_CLIENT_TOOL_VERSION_INVALID`: run `yydra setup .` to restore the exact
  project-local Orval dependency from the committed npm lock. Do not change
  Public API source to disguise a missing or wrong tool.
- `API_CHECK_EXECUTABLE_UNAVAILABLE`: restore or invoke the exact Yydra CLI named
  by the Workspace Origin Record; this is not an API source failure.
- `API_GENERATED_DRIFT` and `API_CLIENT_DRIFT`: after the Public API source and
  generator inputs are coherent, use `yydra generate api .` atomically. Do not
  hand-edit the compared output.
- `API_OPENAPI_PROFILE_INVALID`, `API_OPENAPI_OPERATION_ID_INVALID`,
  `API_OPENAPI_CONTENT_TYPE_INVALID`, `API_OPENAPI_FIELD_NAME_INVALID`,
  `API_OPENAPI_UNKNOWN_FIELD_POLICY_INVALID`, `API_OPENAPI_REQUIREDNESS_INVALID`,
  `API_OPENAPI_DECIMAL_INVALID`, `API_OPENAPI_TIMESTAMP_INVALID`,
  `API_OPENAPI_SAFE_INTEGER_INVALID`, `API_OPENAPI_WIRE_TYPE_INVALID`,
  `API_OPENAPI_NULLABILITY_INVALID`, and `API_OPENAPI_SHAPE_REUSE_INVALID`:
  repair the exact reported wire-profile rule in the Axum/utoipa Public API
  source and its positive and negative contract tests, then generate.
- `API_BREAKING_CHANGE_UNACKNOWLEDGED`: review the reported compatibility change.
  Change the source if it is accidental; if it is intended, record only a real
  review reference through the supported acknowledgement option.
- `API_CLIENT_STAGE_INVALID`, `API_CLIENT_GENERATION_FAILED`, and
  `API_CLIENT_TYPECHECK_FAILED`: inspect the retained generator log and repair the
  named Product-owned generator input, configuration, or Public API source. Do
  not assume the source is wrong before the log identifies it.
- `API_CLIENT_IMPORT_BOUNDARY_VIOLATION` and `API_CLIENT_IMPORT_SCAN_FAILED`:
  repair Product-owned imports so only the handwritten Framework facade reaches
  Generated Client files; do not move handwritten code into generated output.
- `API_RUNTIME_CONFORMANCE_FAILED`: repair the observed server behavior at its
  Product Domain, use-case, persistence, or transport owner; do not edit the
  contract expectation to match a defect.
- An unlisted API code follows the global safe-stop rule; a shared prefix is not
  enough evidence to choose a source, generated, lock, tool, runtime, or
  infrastructure repair.

## Frontend, H5, and accessibility

- `FRONTEND_LOCK_MISSING` and `FRONTEND_LOCK_MUTATED`: restore or deliberately
  regenerate the committed npm lock through the supported dependency path; do
  not let a check rewrite it.
- `FRONTEND_LOCK_INSTALL_FAILED`: inspect the retained npm log first and repair
  the reported configuration, tool, or infrastructure condition, including
  registry configuration, network availability, or authentication. Preserve locked URLs, versions, and integrities;
  do not regenerate the lock unless dependency drift or an explicitly intended dependency change
  is established. An installation failure alone does not establish a bad lock.
- `FRONTEND_TOOLCHAIN_DRIFT`: restore exact package versions from the committed
  lock. Do not change versions to hide an unrelated Product failure.
- `FRONTEND_FORMAT_FAILED`, `FRONTEND_LINT_FAILED`,
  `FRONTEND_TYPECHECK_FAILED`, and `FRONTEND_TEST_FAILED`: repair the exact
  reported Product-owned source or test without loosening the required command.
- `FRONTEND_TEST_DISCOVERY_FAILED` and `FRONTEND_TESTS_EMPTY`: restore canonical
  discoverable tests; zero tests is not success.
- `H5_POSTGRES_UNAVAILABLE`, `H5_POSTGRES_CLEANUP_FAILED`,
  `H5_SERVER_ADDRESS_INVALID`, `H5_SERVER_EXITED`, `H5_SERVER_POLL_FAILED`,
  `H5_SERVER_SUPERVISION_UNAVAILABLE`, `H5_SERVER_TIMEOUT`, and
  `H5_SERVER_UNAVAILABLE`: repair the exact named PostgreSQL, server, address,
  supervision, or cleanup condition and retain its raw phase log.
- `H5_MIGRATION_FAILED`: inspect the migration phase log; do not change H5
  Product Presentation for a database migration failure.
- `H5_E2E_FAILED`: repair the observed end-to-end Product behavior at its actual
  Domain, API, client, or Presentation owner.
- `ACCESSIBILITY_POSTGRES_UNAVAILABLE`, `ACCESSIBILITY_POSTGRES_CLEANUP_FAILED`,
  and `PLAYWRIGHT_CHROMIUM_UNAVAILABLE`: repair or obtain the named required
  infrastructure. Do not edit Product Presentation to disguise availability.
- `ACCESSIBILITY_MIGRATION_FAILED`: inspect the retained migration log and repair
  the forward migration, database state, or required PostgreSQL condition it
  identifies. Do not change Product Presentation for a migration failure.
- `ACCESSIBILITY_ASSERTION_FAILED`: repair visible Product Presentation semantics
  and their real assertions. Do not replace a user-facing role, name, state,
  focus, or recovery assertion with an implementation-only selector.
- `ACCESSIBILITY_FOCUSED_OR_SKIPPED`, `ACCESSIBILITY_NO_EXECUTED_TESTS`,
  `ACCESSIBILITY_SPEC_MISSING`, and `ACCESSIBILITY_REPORT_INVALID`: restore the
  canonical unfocused semantic specification or complete Playwright report. Do
  not claim accessibility evidence or change product behavior until the test
  authority itself is valid.

## Native Android

- `NATIVE_GENERATION_CLEANUP_FAILED`: resolve the reported filesystem or process
  cleanup condition. Do not edit authored Expo inputs for a cleanup failure.
- `NATIVE_GENERATION_DIRTY_OUTPUT`: remove only the reported disposable generated
  host after verifying the target; do not delete Product-owned source.
- `NATIVE_GENERATION_INPUT_POLICY_FAILED` and
  `NATIVE_GENERATION_MUTATED_AUTHORED_INPUTS`: repair the reported authored
  `app.json`, exact dependency, declared config plugin, or local Expo Module so
  generation obeys the input policy and never mutates authored source.
- `NATIVE_GENERATION_FAILED`, `NATIVE_GENERATION_INVENTORY_FAILED`,
  `NATIVE_GENERATION_NONDETERMINISTIC`, and `NATIVE_GENERATION_OUTPUT_MISSING`:
  inspect both retained generation attempts and repair only the identified
  authored generator input or required tool condition. Never patch
  `frontend/android` as an authority.
- `ANDROID_RELEASE_BUILD_FAILED`: inspect the generated Gradle wrapper log, then
  express an identified correction in authored inputs before clean generation.
- `ANDROID_RELEASE_OUTPUT_MISSING` and `ANDROID_RELEASE_OUTPUT_UNREADABLE`:
  verify the exact Gradle task, expected output path, and filesystem condition;
  do not invent an APK or patch the generated host.
- An unlisted native or Android code follows the global safe-stop rule.

## External conditions

For `CHECK_TOOL_UNAVAILABLE`, `CHECK_TOOL_VERSION_UNAVAILABLE`,
`CHECK_TOOL_VERSION_INVALID`, `CHECK_TOOL_VERSION_MISMATCH`,
`CHECK_TOOL_POLL_FAILED`, `DOCKER_UNAVAILABLE`, and
`PLAYWRIGHT_CHROMIUM_UNAVAILABLE`, validate the exact required version, process,
or service and record the unavailable condition. Other codes follow the global
safe-stop rule. Do not report conformance, substitute stale evidence, fabricate
credentials, or convert an infrastructure error into an exception.
