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

The Distribution owns the dependency inventory and known-advisory policy.
`supply-chain.dependencies` records the exact CLI/server Cargo and frontend npm
versions, features, transitives, dependency kinds, targets, and build-tool
exposure. Lock-preserving installation, version consistency and artifact
identity checks remain required.

Under the maintainer-approved scope of Issues #26/#39, dependency-license
admission, source/repository trust review, upstream provenance attestation, and
third-party notice completeness certification are not evaluated. They must not
be represented as passing checks or require individual license approvals.
Existing LICENSE/NOTICE files and attribution are retained; repository licensing,
DCO, Product Workspace ownership and the Workspace Origin Record are unchanged.
Optional declaration metadata is not an independently verified license or source
identity. A declared third-party source commit used for an advisory query does
not prove that locally modified source equals that upstream commit.
If the same locked component has different installed declaration metadata, the
inventory retains per-installation `metadataObservations`; those differences
do not invalidate the component's version and lock-integrity identity.

`supply-chain.advisories` makes one OSV querybatch request for the reported
component identities and requires complete responses and applicable advisory
details. Service failure remains an infrastructure error, not a waiver. A
vulnerability exception must match the advisory, exact component version and
each affected target, with analysis, owner, evidence, approval, expiry and a
re-review trigger. Results cover only the reported query identities at check
time, not unqueried material or unknown/future vulnerabilities.

The declared `frontend/modules/yydra-android-supply-chain/app.plugin.js`
config plugin adds exact Gradle constraints for
`com.google.code.gson:gson@2.14.0` and
`commons-io:commons-io@2.22.0`. Those constraints replace vulnerable Android
runtime transitives affected by GHSA-4jrv-ppp4-jm57 and
GHSA-gwrp-pvrq-jmwv; they are Product-owned generated-host input, not an
exception or a claim about future advisories. Change them only with a fresh
release-runtime resolution, artifact inventory, and advisory query.

After the same check invocation builds and exercises the supported CLI, server,
production H5 Application Surface and Android outputs, the release inventory
retains target-specific CycloneDX SBOMs, artifact paths, checksums and build/test
evidence under `artifacts/supply-chain.release-artifacts/<target>/`.
Dependency graph membership and actual artifact entries are distinct facts;
neither is a claim of independently verified upstream provenance.

Android evidence records resolved release-runtime dependencies, selected
variants and artifact hashes, APK entries including each native library's
decompressed hash, and a separate one-attempt OSV result for reported Maven
identities. Native entries without upstream producer attribution remain
unattributed; no arbitrary matching AAR is promoted to a verified producer.
License/source reviews and notice-completeness checks are outside the current
scope, including the earlier exact native-runtime review requests.
Android material inventory schema 4 distinguishes `runtime-build-input` from
`dependency-graph-only`; neither label asserts that the component was shipped.
APK native entries carry their own paths and hashes without invented producer
edges or entry-level advisory results. Hermes maps may omit `file`: the captured
Gradle task/output paths and bundle/map hashes still bind the retained outputs,
and the bundle hash must match the APK entry. A contradictory `file` fails.

Retained H5 bundles and source maps identify the same build outputs.
Export preparation validates Metro's unique map URL and matching debug ID before
filling an omitted `file` declaration, before browser tests and hashing;
contradictory declarations fail and bundle bytes remain unchanged.
Available build metadata can relate npm inputs to exported bundles without
implying source authenticity or license approval. Reports must explicitly state
what was inventoried, queried, and not evaluated. Full clean and Reading Queue
packaged-consumer acceptance is still required; focused nodes are diagnostic.

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
Gradle invocation for assembly, dependency resolution, and material capture.
It limits Gradle to one worker, compiles Kotlin in the same bounded process,
and applies one-slot CMake compile and link pools to every generated Android
module. Cache seeds, retained files, archives, and source maps have explicit
file/count/byte limits; do not run multiple Android release checks concurrently
on a memory-constrained host.
The material inventory covers project, buildscript, dependency-resolution,
and plugin-management repositories and accepts only the exact policy
authorities, including `gradlePluginPortal()`. It hashes the canonical React
Native release task's bundle and source map; final evidence rejects an APK
that does not contain that exact bundle.
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
