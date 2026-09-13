<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Aggregate Capabilities and adopt moon

Status: accepted — 2026-09-13.

The maintainer confirmed repository and newly created Product Workspace coverage,
moon as the daily command entrypoint, and local source integration. They selected
no compatibility for the old setup/dev/build commands and confirmed the remaining
directory, source-consumer, toolchain, and database boundaries.

## Ownership and delivery

Aggregate authentication under `capabilities/auth/{rust,expo}` and client settings
under `capabilities/client-settings/typescript`. Keep independent Cargo/npm
packages and the existing names. Package-owned authentication lifecycle tests
move out of the product template and gain independent test/typecheck commands.
Client settings remains a TypeScript capability; no artificial Rust package is
introduced. Capability identity remains semantic, as defined in `CONTEXT.md`.

Product pages, navigation, settings definitions, storage adapters, authorization,
and migration history remain product-owned. The existing Android network-policy
plugin configures the entire application, including local cleartext access, and
remains in the template. Directory aggregation does not transfer that policy to
the authentication library.

Ordinary generated products continue to consume the exact registry versions and
integrities selected by their Distribution. Moving sources or adding tasks does
not publish packages. Changed package archives still need a new development
version before publication; public publication remains a separate release action.

## Task orchestration

Use moon **2.5.4**, enforced by `versionConstraint`, in the repository and every
new product. A root repository project executes Cargo workspace commands; npm
packages have their own projects, and `auth:check` aggregates both runtimes.
Each generated product is a separate moon workspace. Framework template files
are not discovered as an instantiated product.

Remove public `yydra setup`, `yydra dev`, and `yydra build`. Product moon tasks
call hidden `yydra internal` execution primitives. Keep creation, diagnostics,
and product-specific database commands public. The executor never calls moon.
This preserves the existing process supervision and Android builder without
copying them into product scripts or maintaining two user-facing workflows.

Use installed Rust/Node/npm/JDK/Android SDK tools. Preserve rolling Rust nightly
and explicit toolchain updates. Disable moon's implicit dependency installation
and configuration synchronization; run dependency setup explicitly with locked
Cargo/npm installation. CI uses moon to execute the same release CLI build and
default CLI tests; DCO and the existing check name remain unchanged.

Initially disable moon result caching for these tasks, preserving the underlying
tools' own incremental builds. Cargo `target`, generated API paths/symlinks,
source-consumer tasks, migrations, dev, and Android are not portable moon outputs.
Frontend-related tasks serialize access to generated artifacts within a moon
pipeline. Any future result caching must first prove input coverage and recovery
after outputs are deleted, including external source paths and environment.

API generation stays with `api-build`, `yydra-build`, and `prepare-api.mjs`.
The existing npm prehooks continue to own API preparation. Default production
build remains backend plus H5; Android remains explicitly selected.

## Explicit source consumers

`moon run repo:source-create -- /absolute/disposable/path` creates a new development
consumer outside the framework checkout. It references this checkout's canonical
Capability sources, records the selected framework root and Distribution version,
and generates locks for that source graph. Subsequent setup honors those locks.
The generated moon tasks use the current checkout's built CLI. Regenerate the
consumer when its template or source dependency graph changes; ordinary source
edits use the existing compiler/bundler behavior. Rust changes take effect on the
next build/restart; no backend file-watching contract is added.

Doctor explicitly recognizes this mode and verifies the declared source paths
and locked package identities. Registry-mode products retain registry checks.
Local source results cannot establish package-delivery correctness; independently
packed libraries and a registry consumer require separate acceptance.

Arbitrary product link/unlink, automatic upgrade, recovery of existing product
edits, and global npm links are outside the selected scope. Each task owns its
consumer, node_modules, target, generated API, and native output; dependency
download caches may be shared. Metro watches the selected source packages and
resolves runtime dependencies from the product to avoid duplicate React copies.

## Services and validation

Start a development database explicitly with `product:db-up`, which waits for
PostgreSQL health. `product:dev` uses `DATABASE_URL`, runs migration first, then
supervises backend and H5. Ctrl-C ends those child processes; database lifetime
remains explicit. External PostgreSQL is supported. The existing dev executor
does not wait for HTTP readiness before starting the frontend; readiness probes
remain part of integration acceptance, not a new dev behavior claim.

Validate independent packages, removed public commands, explicit source identity,
packaged CLI creation, source edits taking effect, registry/package consumption,
API output/link recovery and concurrent task safety, H5, moon cancellation and
failure cleanup, and a real Android release build. Report actual results and
limitations; selected checks do not establish a complete release.

This amends the command-entrypoint parts of ADRs 0004 and 0009, directory and
development-consumption parts of ADRs 0010 and 0011, and the command spelling of
ADR 0006's CI workload. Their historical acceptance remains unchanged.

## Sources

- [moon Rust handbook](https://moonrepo.dev/docs/guides/rust/handbook): Cargo integration and target-cache limits.
- [moon project configuration](https://moonrepo.dev/docs/config/project): task options, mutexes, and persistent tasks.
- [moon workspace configuration](https://moonrepo.dev/docs/config/workspace): project discovery and version constraint.
- [Cargo dependency overrides](https://doc.rust-lang.org/cargo/reference/overriding-dependencies.html): explicit local dependency graphs.
- [Expo monorepos](https://docs.expo.dev/guides/monorepos/): linked packages and runtime dependency resolution.
