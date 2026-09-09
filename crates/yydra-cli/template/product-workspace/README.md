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

## Supported local path

`frontend/.npmrc` selects the same registry used by the committed npm lock, so
setup does not depend on a developer's user-level registry setting. It does not
enable arbitrary remote URL dependencies or change locked versions/integrities.
Treat this file as Product-owned configuration alongside the frontend manifests.

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
duplicate concurrent next-page calls are suppressed. The versioned URL-safe
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

State ownership stays explicit: TanStack Query owns server state, Expo Router
URLs own status/sort state, component state owns the short-lived create form,
and Product Domain rules stay in Rust. No default persistent or global Product
store is installed. The Reading Queue screen separately renders initial and
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

The supported quality entrypoint is read-only for authored, snapshot,
committed-generated, lock, migration, and configuration inputs:

```console
yydra check .
yydra check . --comparison-base main --node database.migration-history
yydra check . --fixture clean --evidence-dir /absolute/external/clean-evidence
```

It emits one result model as human output or versioned JSON Lines and writes a
manifest plus raw node logs to a private, unique system-temporary directory
outside this Workspace by default. An explicit `--evidence-dir` must also be
outside the Workspace and have no symlink ancestor. A focused `--node
<stable-id>` run is diagnostic and records `complete=false`; it is not a
complete core-graph claim. Every invocation embeds the exact
Distribution-owned catalog and executor digests, stable diagnostic vocabulary, prerequisite
graph, result states, retry policy, exception policy, proof boundaries, and
per-node attempts in its manifest and `artifacts/check-catalog.json`.
Missing required infrastructure is reported separately from semantic failure,
and a failed prerequisite skips only its dependent nodes while independent
nodes continue. Semantic, generation, and conformance nodes never retry;
Docker and Playwright browser establishment may retry once, with both attempts
recorded. This Distribution's exception policy is deny-all, so
`.yydra/check-exceptions.toml`, unknown exceptions, and omitted required nodes
fail closed.

A full local pass remains `scope=clean-core-local` with
`aggregateConformance=false`. For aggregate evidence, the Distribution checks
the Workspace Origin Record against exact catalog-owned inputs: `clean` is
`Clean Product` / `clean-product` / `Apache-2.0`, and `reading-queue` is
`Reading Queue` / `reading-queue` / `Apache-2.0`; the option is not a
caller-trusted label. Aggregate conformance requires exactly one
complete, unchanged, uploaded evidence tree for each fixture. The same exact
CLI verifies their catalog, exact executor and Distribution/tool identities, all node and attempt
results, JSON Lines, raw logs, artifact digests, and absence of exceptions:

```console
yydra check \
  --aggregate-evidence /uploaded/clean/manifest.json \
  --aggregate-evidence /uploaded/reading-queue/manifest.json \
  --evidence-dir /absolute/external/aggregate-evidence
```

Missing, malformed, stale, mismatched, unuploaded, symlinked, failed, skipped,
or not-run evidence returns non-zero and cannot produce aggregate conformance.
The repository CI is only an executor of this graph; its complete fixture and
aggregate evidence directories are retained as artifacts.

This Distribution does not evaluate dependency inventories, vulnerabilities or
vulnerability exceptions, SBOMs, or dependency-material attribution. These are
outside the Mechanical Quality Contract and are not reported as passing checks.
The catalog and local/aggregate manifests record this boundary in `notEvaluated`.
Selecting a removed `supply-chain.*` node fails as an unknown node.

Locked installs, exact dependency/tool versions, existing dependency upgrades,
LICENSE/NOTICE and source attribution, Workspace Origin Record, generated
snapshot integrity, and the remaining quality checks stay required. Yydra-owned
npm installations disable automatic audit while still downloading dependencies.
The general deny-all `policy.exceptions` check continues to reject waivers of
remaining quality nodes.

The `yydra-android-dependencies` config plugin retains Gson `2.14.0`, commons-io
`2.22.0`, and the pinned local Bolts Tasks source replacement. Ordinary Android
assembly performs its normal dependency resolution without extra dependency-report
or material-capture tasks. No separate JS bundle/source map analysis material is
required. Server binaries, actual tested production H5 output, APKs, logs, and
hashes remain in their build nodes' evidence and are verified by aggregation.

Check evidence schema 2 and this exact Distribution/catalog/executor identity are
required. Older Workspaces still require their original CLI; no migration or
version override is provided. Historical supply-chain results retain their old
scope and cannot establish conformance to this Distribution.

`database.migration-history` rejects edits or deletions to exact-Distribution
migrations and, when `--comparison-base <git-revision>` is supplied, migrations
present at that Git base; corrections require a new forward migration.
`database.runtime-invariants` proves the selected transaction, rollback,
derived-state, migration, and contention fixtures against real PostgreSQL.
`runtime.post-commit-executor` proves the bounded lossy lifecycle without
claiming durable delivery or business-invariant correctness.
`native.android-generation` runs the reviewed `generate:android` package
script twice from identical authored inputs, records the complete generated
path/mode/byte inventories, rejects drift or authored-input mutation, and
removes the generated hosts. `android.release` repeats clean generation and
uses only the generated Gradle wrapper to assemble an identified release APK:

```console
yydra check . --node native.android-generation
yydra check . --node android.release
```

Both nodes keep raw Expo/Gradle logs and structured inventories or artifact
identity in external evidence. The check command sanitizes its environment, so
Expo/EAS credentials are neither visible nor required. `frontend/android` and
`frontend/ios` are ignored, disposable outputs. Agents may inspect a host with
`npm --prefix frontend run generate:android`, but every fix belongs in the
committed `frontend/app.json`, exact dependencies, a declared standard config
plugin, or a committed local Expo Module; never patch generated Java, Kotlin,
Gradle, manifest, or resource files as source. A pass proves deterministic
generation on the current host and an Android release build only—not Android
runtime, installability on a physical device, native accessibility, store
signing, or bit-for-bit cross-host reproducibility. The release gate uses one
Gradle invocation for release assembly.
It limits Gradle to one worker, compiles Kotlin in the same bounded process,
and applies one-slot CMake compile and link pools to every generated Android
module. Cache seeds and retained artifacts have explicit
file/count/byte limits; do not run multiple Android release checks concurrently
on a memory-constrained host.
Ephemeral runners may provide an absolute public dependency-cache seed through
`YYDRA_GRADLE_DEPENDENCY_CACHE_SEED`. It must contain only a prepared
`modules-2` tree without symlinks, lock files, or `gc.properties`; the gate
copies it into the invocation's account-free Gradle home. Never put user
configuration, credentials, daemon state, or other Gradle home content in the
seed.
`h5.product-presentation-accessibility` executes the visible Product-owned
Playwright role, accessible-name, selected-state, focus, responsive-width, and
dynamic-heading assertions without retry:

```console
yydra check . --node h5.product-presentation-accessibility
```

The node fails closed for a missing or zero-test specification, focused or
skipped tests, malformed JSON evidence, assertion failure, and loss of the
dynamic entry-heading role. It proves only the registered H5 semantics—not
complete WCAG conformance, native assistive-technology behavior, physical
device validation, or unregistered Product behavior.

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
`.yydra/build-support`; `doctor` and `check` verify that snapshot. Targeted backend
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
Generation failure stops the consumer. There is no API generation lock, staging
transaction, or compatibility history. Checks validate the current version.
`yydra check` reuses the preparation script after installing frontend tools and
sets an isolated scratch Cargo target directory for all child processes.
Complete Workspace identity and provenance checks belong to `doctor` and the
appropriate `check` nodes.

The focused production H5 Application Surface acceptance command exports static web assets, serves
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
