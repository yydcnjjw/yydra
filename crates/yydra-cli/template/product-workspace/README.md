<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# __PRODUCT_NAME__

This Product Workspace was created once from Yydra Distribution
`__YYDRA_DISTRIBUTION_VERSION__`. Its product-owned source evolves independently
after creation.

The Workspace Origin Record and `.yydra/distribution-inventory.json` record the
creation inputs, five lifecycle classes, source digests, modes, license notices,
and editing authority. The selected product-source license applies only to new
bytes authored by the Product team. Template bytes copied from Yydra remain
`MIT OR Apache-2.0`; third-party bytes retain their original terms and notices.
Exact-Distribution snapshots and committed generated output are not second
hand-edited authorities.

Creation is a one-shot boundary: there is no template rerun, no template sync,
no upgrade, no compatibility-range selection, and no Distribution-version override contract.

## Supported local path

Install exact dependency graphs and start the pinned PostgreSQL service:

```console
yydra setup .
docker compose up -d --wait postgres
export DATABASE_URL=postgres://postgres:postgres@127.0.0.1:55432/yydra_product
```

Database source and database state have separate explicit commands:

```console
yydra db migration add add_example .
yydra db migrate .
```

Server startup never applies migrations. It fails before listening unless the
database has exactly the compiled versions and checksums. After migration,
`yydra dev .` visibly starts the migration, backend, and H5 frontend phases and
terminates their process groups together on failure or shutdown.

## Reading Queue slice

The initial Product Domain slice creates, lists, completes, and reopens Reading
Queue entries. Its title, source URL, entity-specific identifier, and
queued/completed transition rules live in
`crates/domain` without transport, persistence, React, router, or query-library
dependencies. The concrete create, list, and change-state use cases in
`crates/application` own their SQLx transactions;
`crates/persistence-postgres` contains only the Product Workspace PostgreSQL
operations, and append-only migrations `0002` and `0003` retain final database
constraints. Migration `0004` adds the query-specific status/keyset index.
There is no generic repository or Unit of Work.

The H5 Product Presentation submits `title` and `sourceUrl` through the
handwritten Framework client facade, then completes or reopens entries and
reloads the queue through the same Public API seam. The list uses
`created_at` plus the opaque entry ID as its stable keyset order, bounded pages,
and a nullable `nextCursor`. Status and oldest/newest state live in Expo Router
URLs; TanStack Query keys bind that state, refresh starts at page one, and
duplicate concurrent next-page calls are suppressed. The versioned URL-safe
cursor is signed and bound to status, order, page size, and the reapplied route
authorization context. Tampering or context reuse is a stable 400 Problem.
Pagination makes no cross-request snapshot, total-count, or universal-paginator
promise. Invalid JSON, unknown request/query fields, missing entries, and
prohibited transitions produce stable RFC 9457 Problem types. Mutations never
retry automatically.

The Framework authentication seam declares anonymous and protected routes and
injects credentials through the same client assembly path. The protected
`/api/v1/framework-auth-contract` probe distinguishes a missing credential as
`401` with `WWW-Authenticate` from a rejected credential as `403`; its default
authorized local token is `local-framework-contract`, while the valid but
denied fixture token is `local-framework-forbidden`. These non-secret fixture
values may be replaced through `YYDRA_AUTH_CONTRACT_TOKEN` and
`YYDRA_AUTH_CONTRACT_FORBIDDEN_TOKEN`. This bounded probe is not an Identity
system. Set `YYDRA_READING_QUEUE_CURSOR_SIGNING_KEY` to a stable secret of at
least 32 bytes before starting the server; rotating it intentionally invalidates
previous cursors. Do not place that value in source or logs.

The supported quality entrypoint is read-only for authored, snapshot,
committed-generated, lock, migration, and configuration inputs:

```console
yydra check .
```

It emits one result model as human output or versioned JSON Lines and writes a
manifest plus raw node logs to a private, unique system-temporary directory
outside this Workspace by default. An explicit `--evidence-dir` must also be
outside the Workspace and have no symlink ancestor. A focused `--node
<stable-id>` run is diagnostic and records `complete=false`; it is not a
complete core-graph claim. A full #30 pass remains
`scope=clean-core-local` with `aggregateConformance=false`; cross-fixture CI
aggregation belongs to a later contract. Missing required infrastructure is
reported separately from semantic failure, and a failed prerequisite skips
only its dependent nodes.

Public routes consumed by Generated Client code must be registered through
`product_transport_http::public_routes`. Rust handlers and `utoipa`
declarations are the authored authority; `contracts/openapi.json`,
`frontend/src/generated/public-api/`, and the `.yydra/api-generation*.json`
records are committed outputs and must not be hand-edited. Regenerate the
complete output set through the exact CLI:

```console
yydra generate api .
```

The command exports and lints normalized OpenAPI, runs the pinned Orval
Fetch/TypeScript/Zod stages, and type-checks them in isolated temporary roots.
Only then does it replace the contract, client directory, compatibility
history, and generation record under one Workspace lock and a persisted,
rollback-safe transaction. A read-only check refuses an interrupted
transaction; the next write invocation restores the last complete set before
starting. Use
`yydra generate api . --check` for a read-only comparison. An intentional
lockstep breaking change requires a reviewed, narrow
`--acknowledge-breaking-change <reference>`; the acknowledgment does not make
the change non-breaking. Product code calls the handwritten facade in
`frontend/src/framework/api/`, never the generated directory directly.

The focused production H5 acceptance command exports static web assets, serves
them locally, creates, completes, reopens, filters, paginates, refreshes, and
restores URL state against the real Axum and PostgreSQL service. It verifies
cursor traversal/termination, tamper and context rejection, stable request,
transition, and authentication Problems, plus the focused transaction,
constraint, and keyset-order fixtures. Start the
diagnostic-only server leaf in one terminal:

```console
DATABASE_URL=postgres://postgres:postgres@127.0.0.1:55432/yydra_product \
YYDRA_READING_QUEUE_CURSOR_SIGNING_KEY=<at-least-32-byte-secret> \
  cargo run --locked --bin server
```

Then run the H5 acceptance in a second terminal; do not run it alongside the
Expo development server because both use port 8081:

```console
EXPO_PUBLIC_API_URL=http://127.0.0.1:4000 npm --prefix frontend run test:e2e
```

Add `--message-format=json` before any supported Yydra subcommand for versioned
JSON Lines diagnostics. Cargo, npm, Expo, and Playwright output is forwarded as
diagnostic detail rather than becoming a second supported interface.
