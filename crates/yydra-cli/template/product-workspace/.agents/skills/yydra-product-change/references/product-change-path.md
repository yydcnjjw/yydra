<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Product change path

Load only the sections needed by the current change. Existing Product Workspace
source and tests remain the concrete authority when they are more specific than
this routing guide.

## Product Domain and use case

- Put entities, value objects, states, transitions, and pure invariants under
  `crates/domain`. Keep them free of Axum, SQLx, React, routing, and query-library
  concerns.
- Put typed commands and queries under `crates/application`. The use case
  coordinates the invariant and decides transaction scope.
- Keep correctness-affecting work synchronous in the use case transaction.
  Optional post-commit work may use the bounded lossy executor only after commit;
  it is not durable and cannot uphold a business invariant.

Start with domain and use-case unit tests. Add a PostgreSQL integration test when
constraints, rollback, locking, concurrency, or query behavior are material.

## Migration and persistence

- Create a sequential forward migration with
  `yydra db migration add <name> .`; never alter a migration that already exists.
- Put concrete SQLx operations under `crates/persistence-postgres`.
- Let persistence functions borrow the use case transaction executor. Do not
  create a generic repository or hidden Unit of Work.
- Add database constraints for invariants that must survive all writers, while
  retaining the Product Domain rule as the application meaning.

## Public API source and generation

- The Axum and utoipa Rust declarations under `crates/transport-http` are the
  Public API source authority.
- Define stable operation IDs, request and response shapes, wire conventions,
  authorization classification, status codes, and RFC 9457 Problem types.
- Exercise both success and invalid input, state, authorization, and not-found
  behavior in the runtime contract tests.
- Run `moon run product:build-h5` to export, validate and type-check current API build
  outputs. Generate before frontend consumption, including after build cleanup.
- Keep the contract and `@yydra/generated-api` client in the configured Cargo
  build directory. Do not hand-edit or commit them. No history or breaking-change
  acknowledgment is required; this validates the current version only.

## Product Presentation

- Call the API through `frontend/src/framework/api`, not Generated Client files
  or ad hoc global Fetch calls.
- Keep Product UI under `frontend/src/product-presentation`; do not move Product
  rules into components, hooks, or query callbacks.
- TanStack Query owns server state, Expo Router URLs own shareable filter and sort
  state, and local component state owns short-lived interaction state.
- Cover loading, empty success, stale-data and blocking failures, cancellation,
  typed Problems, transport failures, and recovery as applicable.
- Update visible accessibility assertions in
  `frontend/e2e/product-presentation.accessibility.spec.ts`. Assert the actual
  semantic role, accessible name, state, focus, and recovery surface rather than
  an implementation-only selector.
