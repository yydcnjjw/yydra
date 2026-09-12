<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Consolidate Workspace diagnostics in doctor

Status: accepted — 2026-09-12. The maintainer confirmed the implementation scope.

Remove `yydra check` and its quality graph, node selection, comparison-base,
fixture, exception, manifest, and aggregate-evidence interfaces. Use `doctor`
for read-only Workspace identity and dependency-environment diagnosis. Keeping
an additional quality command would preserve the complexity the maintainer
chose to retire. This is an incompatible command and validation-contract change.

`doctor` retains its existing Origin, license, inventory, provenance, and
build-support snapshot verification. It observes the effective nightly Rust
compiler, Cargo, rustfmt, and Clippy. By default it also observes Node/npm;
`--target server` selects backend tools, `--target h5` selects Rust and frontend
tools, and `--target android` additionally probes the configured JDK and Android
SDK. Node/npm versions are observed without adding an exact-version gate that
the existing template does not declare. Android component presence does not
establish compatibility with the exact versions selected by Expo/Gradle.

All required failures are reported before a nonzero exit. Missing frontend
installation and optional Docker/Compose are warnings: doctor can precede
setup, and a Product Workspace may use external PostgreSQL. Android SDK paths
must be explicit because the account-free builder isolates HOME. Probes have
bounded timeouts, cannot implicitly install Rust toolchains, and do not install
dependencies, build applications, run tests, start databases, or create evidence
trees. Structured diagnostics keep the existing CLI JSON Lines format.

`setup` continues locked dependency installation. `build` keeps backend/H5 and
explicit Android artifact production. Extract its Android builder, bounded
Gradle environment, safe dependency-cache reuse, input-mutation detection, and
process cleanup before deleting the old graph. The APK remains in the generated
host; the next build regenerates that host. No generic graph or evidence model
is needed by this builder.

Quality validation uses the project's Cargo/npm tests, formatters, linters,
and builds, with databases and application servers prepared explicitly for
integration tests. The old Distribution-specific dependency-role checks,
comparison-base migration checks, generated-import scanner, exact Baseline Skill
inventory gate, mandatory test-name/zero-test rules, native-generation
comparison, and evidence aggregation retire with the graph. Ordinary tests do
not implicitly reproduce these additional guarantees. Existing doctor snapshot
checks, SQLx applied-migration validation, API-generation validation, and
product tests remain.

This decision amends the check/evidence portions of ADRs 0001–0006 and the
current local-development and release workflow. Release acceptance still
covers packaged creation, real consumer tests, backend/H5, database/browser
integration, and Android; results are reported with actual commands, versions,
outputs, and coverage limits instead of aggregate manifests. CLI CI and DCO
remain unchanged. Historical ADRs, published releases, Wiki evidence, and the
tutorial pinned to an older commit retain their original scope.
