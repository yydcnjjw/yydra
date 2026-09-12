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
installation explicitly with `rustup update nightly`. This Distribution maintains
nightly-only support and declares no stable MSRV. Doctor reports actual installed
tool versions and disables implicit Rust toolchain installation.

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
structured CLI diagnostics for safe focused repair. The Distribution
inventory and digests are authoritative. The Skills have no independent semantic
version, compatibility resolver, upgrade path, or lifecycle, and their presence
does not establish identical client activation, tools, permissions, behavior,
Agent performance, Agent Eval success, or Skill effect.

To build and run only the server and a persistent PostgreSQL database, enter the
new Workspace and run `docker compose up --build --wait`. Its Dockerfile compiles
inside Docker; this deployment path needs no host Rust, Node, or Yydra installation.
See the generated README's **Run the server with Docker Compose** section for
ports, data retention, credentials, updates, and image-only builds.

For local development, use these distinct commands:

```console
yydra doctor ./reader
yydra setup ./reader
yydra doctor ./reader --target android
yydra dev ./reader
yydra build ./reader
yydra build ./reader --target android
```

`doctor` verifies the Workspace Origin Record and existing Distribution
snapshots, and checks the effective nightly Rust compiler, Cargo, rustfmt,
Clippy, Node.js, and npm. `--target server` selects Rust tools, `--target h5`
selects Rust and frontend tools, and `--target android` adds JDK/SDK diagnostics.
Required failures produce a nonzero exit after reporting the other checks.
It reports actual Node/npm versions; dependency compatibility remains governed
by the installed dependencies rather than a new fixed Node/npm version gate.

Doctor can run before setup. Missing `frontend/node_modules` is a warning;
installation remains setup's responsibility. Docker/Compose availability is an
optional diagnostic for the local container path; external PostgreSQL is also
supported. Doctor does not connect to the product database or require a browser.
Android diagnostics require an explicit `ANDROID_HOME` (or compatible legacy
`ANDROID_SDK_ROOT`), honor `JAVA_HOME`, and inspect existing SDK platforms,
build-tools, Platform-Tools, NDK, and CMake. They do not generate a Gradle wrapper
or install SDK components. The real build selects the exact required component
versions; a diagnostic pass alone does not establish a successful APK build.

`setup` installs Cargo/npm dependencies from committed locks. Start PostgreSQL
explicitly, for example with `docker compose -f compose.dev.yaml up -d --wait
postgres`, and supply `DATABASE_URL`. Set `YYDRA_READING_QUEUE_CURSOR_SIGNING_KEY`
to a stable secret of at least 32 bytes before running the backend. `dev`
applies migrations and starts backend/H5; `db migrate` applies migrations alone.

`build` produces the release backend executable and type-checked H5 static
artifacts by default. `--target server`, `--target h5`, and `--target android`
select one artifact. Android uses an account-free environment and retains its
APK under `frontend/android/app/build/outputs/apk/release/`; logs are under
`frontend/.expo/yydra-build/android.log`. A subsequent Android build regenerates
the host. Fix authored Expo inputs, dependencies, plugins, or local modules;
generated native source is disposable. Build reports actual artifact paths.

The public `yydra-build` library validates derived OpenAPI, runs pinned Orval,
and validates the generated TypeScript client. The product's dedicated
`api-build` package calls it from `build.rs`. Its exact source ships in
`.yydra/build-support`, which doctor verifies. Frontend entrypoints prepare and
link its outputs from Cargo's `OUT_DIR`; targeted backend and migration builds
do not require frontend tools. Missing generated files trigger a package-scoped
clean and rebuild; unrelated Cargo caches remain intact.

Add `--message-format=json` before a subcommand for versioned JSON Lines.
Doctor reports `pass`, required `fail`, or optional `warning`, followed by a
summary. It runs bounded read-only probes; tool installation and application
validation are separate operations.

## Project validation

The `check` command, `--node`, `--fixture`, `--comparison-base`, and evidence
aggregation have been removed. Use `doctor` for environment diagnosis and the
owning tools for validation:

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

Run setup before commands that consume frontend dependencies. PostgreSQL tests
require a disposable database and explicit `--ignored`; H5 tests require a
running migrated backend and Playwright Chromium. The generated README contains
concrete commands and cleanup instructions. These tests remain product-owned;
retired graph-only architecture, migration-comparison, generated-import,
native-reproducibility, and evidence-integrity gates are not implicitly provided
by doctor or these commands.

Repository CI builds/tests the CLI; DCO remains separate. Explicit consumer
integration tests retain their ignore reasons. Release acceptance validates a
packaged CLI and fresh consumers through real builds and tests, including
backend/H5, database/browser integration, and Android. Record actual commands,
versions, outputs, and unrun coverage. Environment diagnosis and CLI unit tests
alone do not establish this acceptance.

Android assembly retains one Gradle worker, bounded Kotlin/CMake parallelism,
and an isolated Gradle home. `YYDRA_GRADLE_DEPENDENCY_CACHE_SEED` may identify an
absolute directory containing only a prepared `modules-2` dependency cache;
symlinks, runtime locks, user configuration, and oversized inputs are rejected.
Preserve shared download caches and run resource-heavy builds serially.

Creation does not provide template synchronization, upgrades, or a
Distribution-version override. Historical releases and the tutorial pinned to
an older source commit retain their original commands and validation scope.
