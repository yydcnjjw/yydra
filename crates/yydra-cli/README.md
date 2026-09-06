<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# yydra-cli

`yydra-cli` installs the exact-version `yydra` executable for creating and
diagnosing Product Workspaces initialized from a Yydra Distribution and owned
independently by their product teams.

Install an exact release while preserving its packaged lockfile:

```console
cargo install yydra-cli --version 0.1.0 --locked
```

Create once with one normalized product-source license choice:

```console
yydra new ./reader \
  --product-name "Reader" \
  --product-id reader \
  --product-source-license Apache-2.0
```

The choice governs only source newly authored by the Product team. Copied Yydra
bytes remain `MIT OR Apache-2.0`, third-party bytes retain their original terms
and notices, and both complete Yydra license texts ship with the package and the
new Workspace. `.yydra/distribution-inventory.json` records lifecycle,
provenance, mode, digest, license-notice authority, and edit authority.

Continue only through supported commands:

```console
yydra setup ./reader
DATABASE_URL=postgres://postgres:postgres@127.0.0.1:55432/yydra_product \
  yydra db migrate ./reader
DATABASE_URL=postgres://postgres:postgres@127.0.0.1:55432/yydra_product \
YYDRA_READING_QUEUE_CURSOR_SIGNING_KEY=<at-least-32-byte-secret> \
  yydra dev ./reader
yydra generate api ./reader
yydra check ./reader
```

`setup` consumes both committed locks. Migration creation (`yydra db migration
add`) and application are explicit; the server only verifies that PostgreSQL
matches its compiled history. `check` is read-only for Workspace inputs and
runs the Distribution-owned core graph, including Public API/Generated Client
drift and the real PostgreSQL/Axum/H5 Reading Queue
create/list/complete/reopen/filter/keyset-pagination path with restorable URL
state, stable cursor Problems, and bounded authentication semantics. The
graph also repeats clean Expo Continuous Native Generation and assembles an
identified Android release APK with the generated Gradle wrapper. These nodes
run from the committed `app.json`, exact `package.json`/`package-lock.json`,
declared config plugins, and local Expo Modules only; the sanitized build
environment does not expose Expo or EAS credentials. `frontend/android` is
ignored disposable output and is removed by `yydra check`; raw Expo/Gradle
logs, both generation inventories, the generated-host inventory used for the
build, and the APK identity are retained in external evidence. Passing proves
deterministic generation on the current host and an account-free release
build, not Android runtime, installation, physical-device behavior, native
accessibility, or store signing. The Product Domain rules remain pure Rust,
while the concrete application use
cases explicitly own SQLx transactions and database constraints remain the
final defense. The Reading Queue transition uses one named cross-domain
transaction to update correctness-affecting progress synchronously under the
selected `READ COMMITTED` row-lock strategy. Optional post-commit work uses a
named, bounded, traced, non-durable executor: it rejects excess admission,
anchors end-to-end task deadlines at admission, never retries, and cannot carry
a business invariant. Existing
migrations are append-only; passing `--comparison-base <git-revision>` to the
focused `database.migration-history` check also rejects edits and deletions
relative to the requested Git base. Focused `database.runtime-invariants` and
`runtime.post-commit-executor` nodes retain discriminating failure fixtures.
`generate api` is the only supported
write path for normalized OpenAPI and the Orval Fetch/TypeScript/Zod outputs;
it validates every isolated stage before a Workspace-locked, journaled,
rollback-safe replacement, and records a replayable compatibility history.
Its read-only form is `yydra generate api ./reader --check`. The default check
evidence directory is a private, unique system-temporary
directory outside the Product Workspace; an explicit `--evidence-dir` must
also be outside the Workspace and have no symlink ancestor. Diagnostic
`--node` selection is explicitly incomplete. A full result is scoped as
`clean-core-local` and explicitly does not claim aggregate conformance. Every
result embeds the exact Distribution catalog and executor digests, prerequisite graph,
stable diagnostics, result states, proof boundaries, deny-all exception
policy, and every execution attempt. Semantic, generation, and conformance
nodes do not retry; infrastructure establishment may retry once and records
both attempts. Add `--message-format=json` before the subcommand for versioned
JSON Lines diagnostics.

CI builds and uploads one exact executor, then uses those same bytes to create
independent clean and Reading Queue Workspaces. The Distribution validates
`clean` as `Clean Product` / `clean-product` / `Apache-2.0` and
`reading-queue` as `Reading Queue` / `reading-queue` / `Apache-2.0` from each
Workspace Origin Record; `--fixture` is not a caller-trusted label. CI uploads
the entire evidence trees, including hidden files, before that executor can grant
aggregate conformance:

```console
yydra check ./clean --fixture clean --evidence-dir /external/clean-evidence
yydra check ./reading --fixture reading-queue --evidence-dir /external/reading-evidence
yydra check \
  --aggregate-evidence /uploaded/clean/manifest.json \
  --aggregate-evidence /uploaded/reading-queue/manifest.json \
  --evidence-dir /external/aggregate-evidence
```

The aggregate verifier requires both exact current catalogs and executor
identities, full
pass node sets, JSON Lines, raw logs, and artifact digests. Missing, malformed,
stale, mismatched, unuploaded, symlinked, failed, skipped, not-run, or excepted
evidence fails closed. Neither local nor aggregate evidence proves macOS/iOS,
native runtime, physical-device behavior, native accessibility, Agent
performance, Baseline Skill effect, or any narrower per-node non-claim.

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

Created Product Workspaces declare a local Expo config plugin that applies
exact Android runtime constraints for `gson@2.14.0` and
`commons-io@2.22.0`, replacing the vulnerable transitives covered by
GHSA-4jrv-ppp4-jm57 and GHSA-gwrp-pvrq-jmwv. These are reviewed dependency
upgrades, not vulnerability exceptions; the resolved release graph and OSV
result remain authoritative on every check.

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

Run the focused Android evidence path through the supported quality entrypoint:

```console
yydra check ./reader --node native.android-generation
yydra check ./reader --node android.release
```

For generated-host diagnosis only, `npm --prefix frontend run
generate:android` leaves the ignored `frontend/android` tree available for
inspection. Express every fix in `app.json`, exact dependencies, a declared
config plugin, or a local Expo Module, then delete and regenerate the host;
never patch generated Java, Kotlin, Gradle, manifest, or resource files as an
authority.

The Android release gate uses one Gradle invocation for assembly, dependency
resolution, and material capture. It limits Gradle to one worker, compiles
Kotlin in the same bounded process, and applies one-slot CMake compile and link
pools to every generated Android module. Its cache seed, retained files,
archives, and source maps have explicit file/count/byte limits. Do not run
multiple Android release checks in parallel on a memory-constrained host.

An ephemeral runner may set `YYDRA_GRADLE_DEPENDENCY_CACHE_SEED` to an absolute
directory containing a prepared `modules-2` cache. The gate copies that cache
into its account-free Gradle home before the build and rejects symlinks, lock
files, `gc.properties`, relative paths, and unsupported entries. Prepare the
seed by copying only Gradle's public dependency cache; never include user
configuration, credentials, daemon state, or other Gradle home content.

Creation does not establish a template rerun, synchronization, upgrade,
compatibility-range, or Distribution-version override contract.
