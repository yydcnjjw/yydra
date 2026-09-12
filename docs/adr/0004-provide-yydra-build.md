<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Provide reusable API generation through yydra-build

Status: accepted — 2026-09-09. Implemented in Distribution 0.4.0 and locally verified.

The maintainer confirmed the consolidated architecture, command behavior, and
acceptance boundary with “确认共同理解”. The design discussion is complete.

Provide a Yydra-owned `yydra-build` library usable by Product Workspace build
scripts. The product supplies its derived Public API Contract; the shared library
owns reusable generation behavior without depending on any particular product.
The selected library scope covers the complete API generation pipeline:
OpenAPI output and profile validation, Orval client generation, TypeScript
checking, generated-client validation, and generated package metadata. Linking
the generated package into frontend dependencies remains a frontend preparation
step. Executing the complete pipeline requires the project-local frontend tools.
The build script belongs to a dedicated product `api-build` crate. The public
build entrypoint will be `yydra build`; API generation is an internal dependency
step, and the public `yydra generate api` command is removed. The selected
default build targets are the backend executable and H5 static artifacts;
Android is selected explicitly. The consolidated contract below is the accepted
implementation scope.

The current contract comes from compiled `transport-http` route declarations.
A downstream package can use `transport-http` and `yydra-build` as build
dependencies and pass the contract to the helper. Moving the existing exporter
invocation into `transport-http`'s own build script would create a dependency
cycle. At decision time, this product integration assessment was based on the
dependency graph without a compiled product prototype. Subsequent integration
and acceptance are recorded in the implementation notes and validation record.

The selected dedicated product `api-build` package makes API generation an
explicit build target: targeted backend builds and migrations do not select
that sibling package, whereas whole-Workspace checks still include it. Putting
the script in `server` was not selected because that package also owns the
`migrate` binary and would bring frontend-tool prerequisites into its build
lifecycle. The generator requires a new permitted architecture role.
`build` must deliver the selected application artifacts; renaming API-only
generation to `build` would not establish that behavior.
The architecture checker must explicitly recognize `yydra-build` as a public
build dependency instead of rejecting it as a framework-internal dependency.

Frontend development, tests, and exports should reuse an internal preparation
script that builds `api-build` and links its output. They must not invoke the
full public application build from an export prehook, which would recurse when
that build itself invokes the frontend export. Defaulting to backend plus H5
keeps the Android SDK, JDK, and Gradle prerequisites specific to an explicitly
selected Android build.

A bounded offline Cargo 1.97.1 probe confirmed that `build-script-executed`
JSON messages expose `out_dir` both after execution and on a fresh build, as
documented by [Cargo](https://doc.rust-lang.org/cargo/reference/external-tools.html#build-script-output).
It also rejected using rewritten output files as `rerun-if-changed` inputs:
with a 150 ms write delay, successive unchanged builds kept rerunning. The
selected integration therefore tracks real inputs, keeps outputs within
`OUT_DIR`, and handles missing outputs by cleaning and rebuilding only the
product generator package from the outer preparation step. Frontend links are
restored independently. The probe informed the recovery design; it was not Product Workspace
acceptance evidence. The implementation is separately verified with real Orval. Probe records are in
`/tmp/yydra-buildrs-probe-hvjeitpl/`.

## Implementation contract

The selected architecture and the following concrete defaults are accepted.
Implementation details and Product Workspace acceptance are recorded below.

- `yydra build [workspace]` builds the backend in release mode, type-checks the
  frontend, and exports H5 production artifacts. `--target server`,
  `--target h5`, and `--target android` select an application artifact
  explicitly; this flag is not a Rust target triple. Android type-checks the
  frontend and produces the existing account-free release APK. The command
  reports actual artifact paths and fails if a required step fails.
- Generation and frontend preparation happen automatically wherever the
  Generated Client is consumed, including development and test entrypoints.
  `yydra build` orchestrates the selected artifact builders and reuses these
  prerequisites. It does not apply database migrations, start services, publish
  artifacts, or stand in for the full Mechanical Quality Contract.
- `yydra-build` accepts the caller's derived OpenAPI and explicit generation
  configuration. Product-specific declarations remain in `transport-http`.
  The product `api-build/build.rs` is the thin build-dependency adapter. The
  former exporter executable can be removed after all consumers use this path.
- Keep API outputs in an owned subtree of `OUT_DIR`. Identify the selected
  package's output through Cargo JSON messages. Use Cargo's change detection
  for Rust build dependencies and explicitly tracked generator configuration
  and tool inputs. Do not track self-rewritten generated files as inputs or
  introduce another hash ledger, lock, staging transaction, or cache database.
- Outer preparation verifies required output files before linking. If Cargo
  reports a successful build with missing outputs, clean only the selected
  `api-build` package and rebuild once. A failed generator or failed rebuild
  fails the command; it is not automatically retried in a loop. A missing
  frontend link is repaired without forcing generation. Other packages' Cargo
  dependency caches remain intact.
- Preserve current OpenAPI/client/type checks and narrow functional build
  prerequisites. Full provenance and Distribution snapshot checks remain in
  `doctor` and `check`. Teach the architecture rules about the generator role
  and specifically permitted public build dependency. Reorder whole-Workspace
  check prerequisites so frontend tools are ready before compiling the new
  generator member, while retaining original authored-input immutability.
- Deliver the new framework crate, CLI, product templates, dependency locks,
  diagnostics, and guidance together in a subsequent Distribution. Existing
  Product Workspaces are not automatically migrated. The separately proposed
  rolling-nightly change is outside this implementation scope.

## Acceptance boundary

Verify the packaged CLI and packaged `yydra-build` together through a fresh
Product Workspace. Cover the default backend/H5 build and an explicit real
Android APK build; targeted backend compilation and migrations must not select
the generator or require its frontend tools. Verify clean creation, unchanged
input reuse without repeatedly running Orval, changed-input regeneration,
missing-output recovery, and relinking after removal of the frontend link.
Invalid contracts, incompatible tools, and generator failures must block their
consumers and succeed only after repair. Retain custom Cargo target-directory
and sequential shared-cache coverage, current API/runtime conformance tests,
and authored-source/dependency-lock immutability. Run the updated complete
`yydra check` contract and record the actual evidence boundary.

This decision amends the generation entrypoint and output placement from
[ADR 0002](0002-simplify-api-generation.md), while retaining its sequential
workflow, disposable
outputs, current-version validation, and dedicated provenance checks. The
selected output and frontend integration follows Cargo's recommendation that
scripts write within `OUT_DIR` and accounts for successful build scripts not
necessarily rerunning on every invocation. See the
[Cargo build-script reference](https://doc.rust-lang.org/cargo/reference/build-scripts.html).

## Implementation notes

Distribution `0.4.0` introduces `crates/yydra-build`. The CLI carries its canonical
source through relative template-file symlinks, which Cargo flattens into regular
files when packaging. Creation writes this exact source snapshot under
`.yydra/build-support`; the product excludes it from Workspace membership and
uses an exact-version path build dependency. This delivers a buildable candidate
without requiring registry publication or maintaining duplicate helper sources.
The independently packaged helper and bundled source are byte-compared during
acceptance. Both retain the Distribution's complete license texts. Snapshot
verification remains in `doctor` and `check`.

Real Cargo tests confirmed that same-named product packages in different
checkouts can reuse one `OUT_DIR` with a shared target directory. The helper
therefore owns `OUT_DIR/yydra-api/<canonical-frontend-path-key>/`. This key only
separates directories; Cargo remains the incremental-build authority. The build
script reads the invocation's `CARGO_MANIFEST_DIR` at runtime. The generated npm
package's standard `files` metadata lets preparation detect missing generated
files without maintaining a separate hash ledger.

The independently packaged helper and CLI passed fresh Product Workspace
acceptance, including default backend/H5, explicit Android, Cargo incremental
and recovery behavior, and the complete 29-node quality contract. See the
[validation record](https://github.com/yydcnjjw/yydra/wiki/Validation-2026-09-09-Yydra-Build/6a878e624ef5fba3d3069e3daf38a7169d798982) for package identities,
test results, review fixes, retained evidence, and the local acceptance boundary.

## Subsequent diagnostic and validation scope

[ADR 0009](0009-consolidate-diagnostics-in-doctor.md) retires `yydra check`,
the quality graph, and aggregate evidence. Current environment diagnostics use
`doctor`; project validation uses explicit Cargo/npm tests and builds. The
original decision and dated validation above retain their historical scope.
