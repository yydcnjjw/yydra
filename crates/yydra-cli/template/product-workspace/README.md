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

Exactly two portable Baseline Skill snapshots are materialized under
`.agents/skills`: `yydra-product-change` routes an end-to-end Product Domain
change, and `yydra-diagnose` interprets structured `doctor` and `check` results
for safe focused repair. Their exact Distribution inventory and digests are the
authority; the Skills have no independent version, compatibility resolver,
upgrade path, or lifecycle. Client discovery is only a thin integration seam and
does not establish identical activation, tools, permissions, behavior, Agent
performance, Agent Eval success, or Skill effect.

Creation is a one-shot boundary: there is no template rerun, no template sync,
no upgrade, no compatibility-range selection, and no Distribution-version override contract.

## Run the server with Docker Compose

On a machine with Docker Engine and the Compose plugin, run from this Product
Workspace (the directory containing this README):

```console
docker compose up --build --wait
curl --fail http://127.0.0.1:4000/health
```

The multi-stage Dockerfile compiles the release `server` and `migrate` binaries
with Rust nightly inside the builder. The runtime image contains the binaries,
entrypoint, and runtime dependencies. Host Rust, Node, npm, and the Yydra CLI
are not required for this path; Docker needs access to the image, Rust, and
Cargo download services during the initial build. H5 and Android are separate
build targets and are not included in this server image.

Compose initializes random database and cursor-signing credentials once in the
`credentials` volume, starts PostgreSQL with the `postgres-data` volume, runs
the image's migration executable, then starts the backend. Failed initialization
or migration prevents initial backend startup. `--wait` returns success only
when PostgreSQL and the backend pass their health checks. The backend still
verifies compiled migration history; the server executable does not apply it.
The runtime server runs as a non-root user and handles Docker's stop signal.

The API binds to host loopback on port 4000 by default. To choose a port or
bind to an externally reachable host interface, set `YYDRA_SERVER_PORT` or
`YYDRA_SERVER_HOST` (for example, `0.0.0.0`) in the shell or a local `.env` file
before `compose up`. PostgreSQL has no published host port. The template's
Reading Queue is anonymous and its protected endpoint is only an authentication
contract fixture, not a product identity system; configure product access
control and HTTPS before exposing an actual product publicly.

```console
docker compose logs --tail 100 server migrate
docker compose stop
docker compose start --wait
docker compose down
docker compose up --build --wait
```

`stop`, `down`, container replacement, and rebuilding the image preserve the
named volumes. Keep both volumes together when backing up or moving a deployment;
losing credentials is not automatic database-password recovery. `down --volumes`
explicitly deletes credentials and database records. Do not use it for ordinary
updates. After pulling a source change, run `up --build --wait` to rebuild and
apply forward migrations. This is a single-host deployment with possible downtime,
not an atomic upgrade or an automatic rollback of application or database changes.
Migration failure on an update may leave the previous server running or unhealthy;
inspect the migration logs and repair forward before rerunning the command.

For image-only packaging use `docker compose build server`. The image defaults to
`__PRODUCT_ID__-server:local`; `YYDRA_SERVER_IMAGE` overrides its local tag. To use
the image independently of Compose, provide `DATABASE_URL` and
`YYDRA_READING_QUEUE_CURSOR_SIGNING_KEY` at runtime, run its `migrate` command
against that database first, then start its default `server` command. Credentials
are runtime inputs and never build arguments. Publishing the image to a registry
is a separate operation. The container files are Product-owned and may be adapted
to the product's deployment environment.

Development uses the separate `compose.dev.yaml` below, with its own default
Compose project name (`__PRODUCT_ID__-dev`, versus `__PRODUCT_ID__-server`). It deliberately retains
an ephemeral PostgreSQL instance and the local port expected by `yydra dev` and
checks; its data is unrelated to the deployment volumes.

## Supported local path

`frontend/.npmrc` selects the same registry used by the committed npm lock, so
setup does not depend on a developer's user-level registry setting. It does not
enable arbitrary remote URL dependencies or change locked versions/integrities.
Treat this file as Product-owned configuration alongside the frontend manifests.

Rust uses the rolling `nightly` channel with rustfmt and Clippy. Refresh a local
installation explicitly with `rustup update nightly`; builds and checks use the
installed channel without adding an update step. This Distribution maintains
nightly-only support and declares no stable MSRV. Doctor reports the actual
installed Rust versions and disables implicit toolchain installation.

Install exact dependency graphs and start the pinned PostgreSQL service:

```console
yydra setup .
docker compose -f compose.dev.yaml up -d --wait postgres
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
constraints. Migration `0004` adds the query-specific status/keyset index, and
`0005` adds the synchronous Reading Progress projection.
There is no generic repository or Unit of Work.

The named `ChangeReadingEntryStateAndRecordProgress` orchestration use case
owns one transaction for the entry transition and its correctness-affecting
progress value. Persistence functions borrow that transaction executor; any
failure rolls the complete command back, and success commits explicitly.
PostgreSQL stays at `READ COMMITTED`; this demonstrated invariant combines the
database constraints and conditional progress update with a row lock, and the
concurrent fixture exposes one conflict without command retry.

`crates/application/src/post_commit.rs` is a separate, bounded and non-durable
seam for optional work submitted only after an authoritative commit. It accepts
stable named lossy tasks, rejects excess capacity, anchors each end-to-end task
deadline at admission so queue wait consumes it, records structured tracing and
metrics, never retries, and distinguishes task
failure, timeout, cancellation, panic, and deadline-bound shutdown. Process
crash can lose admitted work. Business invariants therefore remain synchronous;
this seam is not an outbox, queue, worker, or delivery guarantee.

The H5 Product Presentation submits `title` and `sourceUrl` through the
handwritten Framework client facade, then completes or reopens entries and
reloads the queue through the same Public API seam. The list uses
`created_at` plus the opaque entry ID as its stable keyset order, bounded pages,
and a nullable `nextCursor`. Status and oldest/newest state live in Expo Router
URLs; TanStack Query keys bind that state, refresh starts at page one, and
duplicate concurrent next-page calls are suppressed. Refresh cancels an in-flight
next page before retaining only the previous first page, and blocks pagination
until the refresh settles. A late cancelled page cannot reappear. After a
successful write, cached filter/order variants are marked stale; the active
list refreshes from page one while its previous first page stays visible. A
failed follow-up read reports that the entry was saved and the displayed list
may be out of date. Recovery retries the read without repeating the write.
The versioned URL-safe
cursor is signed and bound to status, order, page size, and the reapplied route
authorization context. Tampering or context reuse is a stable 400 Problem.
Pagination makes no cross-request snapshot, total-count, or universal-paginator
promise. Invalid JSON, unknown request/query fields, missing entries, and
prohibited transitions produce stable RFC 9457 Problem types. Mutations never
retry automatically.

`frontend/src/framework/runtime.tsx` is the shared assembly seam. Production
creates one Framework client and one `QueryClient`, connects browser or native
online/focus signals, forwards TanStack Query `AbortSignal` values, and emits
sanitized structured failure diagnostics. Queries retry only transport
failures, at most twice with bounded backoff; Problems, cancellation, and
contract violations do not retry. The Test Runtime injects a fake Framework
client and an isolated no-retry `QueryClient` through the same Provider seam;
tests do not mock Generated Client files, global Fetch, query hooks, or Query
internals.

The handwritten API facade and anonymous health client share request execution:
HTTP(S) base URLs resolve from the origin (path, query, and fragment are removed),
and each request has a ten-second default deadline covering credentials,
transport, and response consumption. Caller cancellation remains `cancelled`,
including during body reads; timeout remains `transport`. The runtime's existing
transport-only query retry policy applies to timeouts too. Endpoint validation
and credential policies remain separate: health accepts its existing JSON shape
and ordinary HTTP failures, while generated API operations retain their exact
status, media type, and Problem Details checks. Only generated API operations
receive the configured credential headers.

State ownership stays explicit: TanStack Query owns server state, Expo Router
URLs own status/sort state, component state owns the short-lived create form,
and Product Domain rules stay in Rust. `ReadingEntryForm` owns the whole draft
and its submission revision. Inputs remain editable during submission; success
clears the draft only if it has not been edited since that submission. Editing
either field preserves both fields, including when an edit is later reversed.
`useReadingQueue` owns queries, writes, and refresh coordination; the screen
composes the form, queue panel, and service status. No default persistent or
global Product store is installed. The Reading Queue screen separately renders initial and
background loading, true empty success, blocking and stale-data failures,
cancellation, typed Problem recovery, transport failure, and contract-safe
fallbacks while retaining responsive wrapping, scrolling, focus, and recovery.

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

## Environment diagnostics and validation

```console
yydra doctor .
yydra doctor . --target server
yydra doctor . --target android
```

Doctor verifies Workspace identity and snapshots and probes installed tools.
Default diagnosis covers nightly Rust (with Cargo, rustfmt, and Clippy) and
Node/npm. `--target server` checks backend tools; `--target h5` also checks
frontend tools. `--target android` adds JDK and SDK component diagnostics.
Missing frontend installation and optional Docker/Compose are warnings; setup
owns dependency installation, and external PostgreSQL is supported. Required
failures cause a nonzero exit. Android requires explicit `ANDROID_HOME` (or
consistent `ANDROID_SDK_ROOT`) so the account-free build can find the SDK.
Doctor can run before setup and does not build, test, or install tools.

Use the project's own validation commands after setup:

```console
cargo fmt --all --check
cargo check --locked --workspace --all-targets --all-features
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
cargo test --locked --workspace --all-targets --all-features
cargo test --locked --workspace --all-features --doc
npm --prefix frontend run format:check
npm --prefix frontend run lint
npm --prefix frontend run typecheck
npm --prefix frontend test
```

Database integration tests modify database state. Use a fresh disposable
PostgreSQL database, never a deployment database. For example, run these commands
in the same terminal with an unused port and a task-specific Compose project:

```sh
export YYDRA_POSTGRES_PORT=55439
export DATABASE_URL=postgres://postgres:postgres@127.0.0.1:55439/yydra_product
docker compose -p yydra-db-tests -f compose.dev.yaml up -d --wait postgres
cargo run --locked --bin migrate
cargo test --locked --test reading_queue_postgres -- --ignored --test-threads=1
docker compose -p yydra-db-tests -f compose.dev.yaml down --volumes
```

Always run the final cleanup for that disposable project, including after a test
failure. Each test applies/verifies migrations as needed; serial execution keeps
the tests' database fixtures separate. Run H5 tests against a separately prepared
backend as described below. The CLI no longer owns a quality graph or aggregate
manifest; diagnosis and individual tests prove only their stated scope.

`frontend/android` and `frontend/ios` are disposable generated outputs. Express
native fixes in app configuration, locked dependencies, plugins, or local Expo
Modules. `yydra build . --target android` generates a clean host and retains its
APK and build log. It uses an account-free Expo/Gradle environment with bounded
parallelism; never patch generated native source as an authority. Android
diagnosis checks component presence, while the build selects required versions.
An APK build does not prove device/runtime or accessibility behavior.

An optional `YYDRA_GRADLE_DEPENDENCY_CACHE_SEED` must be an absolute directory
containing a prepared `modules-2` dependency tree, without symlinks, locks, user
configuration, credentials, or daemon state. It is copied into the isolated
Gradle home under explicit size/count limits. Shared download caches remain
reusable; run large Android builds serially on constrained hosts.

Public routes consumed by Generated Client code must be registered through
`product_transport_http::public_routes`. Rust handlers and `utoipa`
declarations are the authored authority. Build application artifacts with:

```console
yydra build .
yydra build . --target server
yydra build . --target h5
yydra build . --target android
```

The default builds the release backend executable, type-checks the frontend, and
exports H5 static assets. An explicit target selects just that artifact. Android
requires the Android SDK, JDK, and Gradle and produces an account-free release APK
under `frontend/android/app/build/outputs/apk/release/`. Build reports the actual
artifact paths. It does not run migrations, start services, or publish artifacts.

The dedicated `crates/api-build/build.rs` obtains the derived OpenAPI from
`transport-http` and calls the public `yydra-build` helper. The helper owns OpenAPI
validation, pinned Orval Fetch/TypeScript/Zod generation, and generated-client
TypeScript validation. Its exact Distribution source ships in
`.yydra/build-support`; `doctor` verifies that snapshot. Targeted backend
and migration builds do not compile `api-build` or require frontend tools.
Whole-Workspace Cargo builds include it and require installed frontend tools.

Outputs live under the generator package's Cargo `OUT_DIR`, in
`yydra-api/<workspace-key>/`. The key keeps outputs separate when projects share
one Cargo cache sequentially. These are disposable build outputs. Only the
handwritten facade in `frontend/src/framework/api/` may import the generated
`@yydra/generated-api` package. Frontend dev, tests, type checking, H5 export, and
native generation run `scripts/prepare-api.mjs` automatically. It builds
`api-build`, reads Cargo's reported output directory, and restores the package
link in `node_modules`; no CLI lookup is required.

Cargo tracks Rust build dependencies, generator configuration, frontend tool
metadata, and PATH. Unchanged inputs reuse the previous generation. If required
outputs are missing, preparation cleans only `api-build` and rebuilds once;
other dependency caches remain. A missing frontend link is repaired directly.
Generation failure stops the consumer. Missing outputs are repaired by the
owning preparation script. Workspace identity and provenance diagnosis belong
to `doctor`; application tests remain explicit project commands.

The focused production H5 Application Surface acceptance command exports static web assets, serves
them locally, creates, completes, reopens, filters, paginates, refreshes, and
restores URL state against the real Axum and PostgreSQL service. It verifies
cursor traversal/termination, tamper and context rejection, stable request,
transition, and authentication Problems. Run the transaction, constraint, and
keyset-order database fixtures separately with the `reading_queue_postgres`
Cargo command above against a disposable database. Start the backend in one terminal:

```console
DATABASE_URL=postgres://postgres:postgres@127.0.0.1:55432/yydra_product \
YYDRA_READING_QUEUE_CURSOR_SIGNING_KEY=<at-least-32-byte-secret> \
  cargo run --locked --bin server
```

Install Playwright Chromium explicitly with `npm --prefix frontend exec -- playwright install chromium`.
Then run H5 acceptance in a second terminal; do not run it alongside the
Expo development server because both use port 8081:

```console
EXPO_PUBLIC_API_URL=http://127.0.0.1:4000 npm --prefix frontend run test:e2e
```

Add `--message-format=json` before any supported Yydra subcommand for versioned
JSON Lines diagnostics. Cargo, npm, Expo, and Playwright output is forwarded as
diagnostic detail rather than becoming a second supported interface.
