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
`clean-core-local` and explicitly does not claim aggregate conformance. Add
`--message-format=json` before the subcommand for versioned JSON Lines
diagnostics.

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

Creation does not establish a template rerun, synchronization, upgrade,
compatibility-range, or Distribution-version override contract.
