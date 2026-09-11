<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# yydra-cli

`yydra-cli` installs the exact-version `yydra` executable for creating and
diagnosing Product Workspaces initialized from a Yydra Distribution and owned
independently by their product teams.

For a step-by-step Chinese introduction on Linux/Bash, follow
[Run your first product with yydra-cli](https://github.com/yydcnjjw/yydra/blob/main/docs/tutorials/yydra-cli-getting-started.md).
It covers packaging and installing the 0.5.0 candidate, creating a Product
Workspace, running the backend and H5 application, and building artifacts.

Distribution `0.5.0` is a local development candidate; it has not been published.
Use the candidate's independently packaged `yydra-cli-0.5.0.crate` and its recorded
checksum, then extract and install with the packaged lockfile in a fresh directory:

```sh
rustup toolchain install nightly --profile minimal --component rustfmt,clippy
sha256sum --check yydra-cli-0.5.0.crate.sha256
tar -xzf yydra-cli-0.5.0.crate
cargo +nightly install yydra-cli@0.5.0 --path ./yydra-cli-0.5.0 --locked
yydra --version
```

The package and installed executor must be the same bytes used for acceptance.
A source-checkout binary does not establish packaged-consumer conformance.
Cargo may download locked dependencies. No registry publication or login is
required. The historical [Distribution 0.1.0 release](https://github.com/yydcnjjw/yydra/releases/tag/distribution-v0.1.0)
and its recorded supply-chain contract remain unchanged; use its exact CLI for
Workspaces it created.

Rust uses the rolling `nightly` channel with rustfmt and Clippy. Refresh a local
installation explicitly with `rustup update nightly`; builds and checks use the
installed channel without adding an update step. This Distribution maintains
nightly-only support and declares no stable MSRV. Checks preserve actual Rust
tool versions, including build identities, and successful runs from different
nightly dates may be aggregated. Evidence applies to the versions actually
validated, while the remaining exact Distribution and tool constraints still apply.

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

Each new Workspace also contains exactly two portable, exact-Distribution
Baseline Skill snapshots under `.agents/skills`: `yydra-product-change` guides
the stable vertical Product Domain change path, and `yydra-diagnose` interprets
structured `doctor` and `check` results for safe focused repair. The Distribution
inventory and digests are authoritative. The Skills have no independent semantic
version, compatibility resolver, upgrade path, or lifecycle, and their presence
does not establish identical client activation, tools, permissions, behavior,
Agent performance, Agent Eval success, or Skill effect.

To build and run only the server and a persistent PostgreSQL database, enter the
new Workspace and run `docker compose up --build --wait`. Its Dockerfile compiles
inside Docker; this deployment path needs no host Rust, Node, or Yydra installation.
See the generated README's **Run the server with Docker Compose** section for
ports, data retention, credentials, updates, and image-only builds.

For local development, start `docker compose -f compose.dev.yaml up -d --wait postgres`
in the Workspace, then continue through the CLI commands:

```console
yydra setup ./reader
DATABASE_URL=postgres://postgres:postgres@127.0.0.1:55432/yydra_product \
  yydra db migrate ./reader
DATABASE_URL=postgres://postgres:postgres@127.0.0.1:55432/yydra_product \
YYDRA_READING_QUEUE_CURSOR_SIGNING_KEY=<at-least-32-byte-secret> \
  yydra dev ./reader
yydra build ./reader
yydra build ./reader --target android
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
`yydra build` produces the release backend executable and type-checked H5 static
artifacts by default. `--target server`, `--target h5`, or `--target android`
selects one artifact; Android uses the same account-free release builder as
`check`. Artifact paths are reported on success. Build does not start services
or apply migrations.

The public `yydra-build` library owns OpenAPI validation, pinned Orval generation,
and generated-client TypeScript validation. The product's dedicated `api-build`
package calls it from `build.rs`. The CLI bundles the exact helper source into
`.yydra/build-support` so the new Workspace builds without registry publication;
this source snapshot has the same version as the separately packaged helper.
Frontend entrypoints privately build that package and link its output from
Cargo's `OUT_DIR`. Unchanged inputs reuse Cargo's result, a missing frontend link
is restored directly, and missing generated files trigger one package-scoped
clean and rebuild. Generation failure stops the consumer. Full identity and
snapshot checks remain in `doctor` and `check`. The default check
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

Repository CI builds `yydra-cli` and runs its default unit and command-behavior
tests; DCO is a separate required check. Consumer integration tests are retained
with explicit ignore reasons. After preparing the required tools and caches,
select them with:

```console
cargo test --locked --package yydra-cli --test <target> <test-name> -- --ignored
```

CLI CI does not establish complete Product Workspace conformance.

Release validation uses one exact executor to create and check independent clean
and Reading Queue Workspaces. The Distribution validates `clean` as
`Clean Product` / `clean-product` / `Apache-2.0` and `reading-queue` as
`Reading Queue` / `reading-queue` / `Apache-2.0` from each Workspace Origin
Record; `--fixture` is not a caller-trusted label. Retain both entire evidence
trees, including hidden files, before that same executor grants aggregate
conformance:

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

The Android release gate uses one Gradle invocation for release assembly. It limits Gradle to one worker, compiles
Kotlin in the same bounded process, and applies one-slot CMake compile and link
pools to every generated Android module. Its cache seed, retained files,
and artifacts have explicit file/count/byte limits. Do not run
multiple Android release checks in parallel on a memory-constrained host.

An ephemeral runner may set `YYDRA_GRADLE_DEPENDENCY_CACHE_SEED` to an absolute
directory containing a prepared `modules-2` cache. The gate copies that cache
into its account-free Gradle home before the build and rejects symlinks, lock
files, `gc.properties`, relative paths, and unsupported entries. Prepare the
seed by copying only Gradle's public dependency cache; never include user
configuration, credentials, daemon state, or other Gradle home content.

Creation does not establish a template rerun, synchronization, upgrade,
compatibility-range, or Distribution-version override contract.
