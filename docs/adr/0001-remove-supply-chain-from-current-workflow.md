<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Remove supply-chain evaluation from the current workflow

Status: accepted — 2026-09-08.

The maintainer has decided that supply-chain evaluation is unnecessary for the
current stage of creating Product Workspaces and samples with Yydra. Remove its
automated scanning, vulnerability exceptions, SBOMs, and dedicated dependency
material collection from subsequent development and validation. Preserve locked
dependencies, existing dependency upgrades, ownership and attribution, generated
authority, normal builds and tests, and the integrity of Conformance Evidence.

The maintainer selected both recommended scope boundaries with “按推荐” and
confirmed the complete shared understanding with “确认共同理解”. The scope and
acceptance conditions are accepted. Local implementation and verification for
Distribution `0.2.0` are complete; see the
[validation record](https://github.com/yydcnjjw/yydra/wiki/Validation-2026-09-08-Supply-Chain-Removal/6a878e624ef5fba3d3069e3daf38a7169d798982).
The decision session changed local domain documentation only. The maintainer
subsequently invoked `/implement`, authorizing implementation and a local commit
under this scope. GitHub publication and historical release/evidence changes remain
outside that authorization.

## Context and historical authority

- The authoritative context map currently identifies one context,
  [Yydra Framework](../../CONTEXT.md). This is a change to its Distribution-owned
  Mechanical Quality Contract, not a new context inferred from CLI directories.
- Source inspected: `6b916aa870ddfefd66bd75af803e5c02a9634a7c`, plus the existing
  uncommitted agent guidance, context map, glossary, and research documents.
- [Issue #26](https://github.com/yydcnjjw/yydra/issues/26) and
  [Issue #39](https://github.com/yydcnjjw/yydra/issues/39) were read on 2026-09-08;
  both are closed. Their 2026-09-05 amendment removed license/source review but
  retained exact dependency inventories, advisories, vulnerability exceptions,
  target SBOMs, and artifact evidence. This decision removes that remaining
  supply-chain workflow from the subsequent contract.
- The [Bolts ADR](2026-09-05-pinned-bolts-source-replacement.md) also retains those
  requirements. This ADR supersedes only its subsequent inventory,
  advisory, material-collection, and SBOM obligations. The actual source
  replacement, original source, LICENSE, attribution, and compatibility tests stay.
- Preserve the old ADR, closed issue bodies/comments, dated research, the
  [0.1.0 release procedure](../releases/0.1.0.md), published packages/tags, and all
  historical successful and failed evidence. They describe their original scope;
  their recorded results are not results for the revised contract.

## Selected scope boundaries

1. **Existing Product Workspaces:** apply the change only to a new exact
   Distribution and newly created Workspaces. Existing Workspaces continue to
   require their original CLI. Introduce no migration, automatic deletion of old
   files, or version override.
2. **Dedicated JavaScript analysis materials:** remove forced collection of
   separate Android JS bundles/maps and H5 source maps, and remove their
   mapping/binding prerequisites. Retain ordinary production outputs, logs,
   identities, and hashes. Application bundles required to run the product remain;
   maps naturally emitted by a build do not become mandatory evidence.

## Removal and preservation boundary

| Area | Remove | Preserve |
| --- | --- | --- |
| Check graph | `supply-chain.policy`, `supply-chain.dependencies`, `supply-chain.advisories`, `supply-chain.release-artifacts`; their dispatch, vocabulary, dedicated reports and tests | Every other required node, dependency edges, failure handling, selected/full-run distinction, and fail-closed aggregation |
| Workspace creation and diagnosis | Two `.yydra/supply-chain-*.json` authorities, their template/lifecycle entries and global required-file comparisons | Workspace Origin Record, exact Distribution/template/catalog/Baseline Skill identity, distribution inventory, generated API authority, source-license declaration and read-only diagnosis |
| Dependency management | Expanded component/feature/target/advisory inventories, OSV queries, vulnerability exceptions and expiry/approval mechanisms, scanning-only data and generator | Cargo/npm locks, exact direct versions/toolchains, locked installs, ordinary dependency resolution and existing upgrades; regenerate locks only for necessary scoped manifest changes |
| Automatic npm audit | Automatic registry advisory requests during Yydra-owned setup, locked installation and CI preparation | Package downloads and lock checks; removal does not promise an offline build |
| Android | Dedicated Gradle dependency-report/material tasks, material init script/environment, repository/material inventories, material summaries and Maven advisory work | Clean repeated CNG, standard Gradle release assembly, generated-host inventory, APK bytes/hash, host/tool identity, account isolation, resource bounds, safe cache reuse and generated-host cleanup |
| Third-party attribution | Scanning and SBOM uses of third-party metadata | Existing LICENSE/NOTICE and source attribution, exact file-level third-party classification, repository SPDX checks, DCO, product source-license selection and Bolts compatibility tests |
| Evidence | Per-target supply-chain copies, SBOMs, component/archive-entry inventories, dependency/source-map linkage, mandatory separate JavaScript bundle/map analysis material | Built server, production H5 output, APK, logs/JSONL, input and artifact digests, exact executor/catalog binding, retained-tree validation and full clean/Reading Queue aggregation |

`policy.exceptions` remains: it rejects attempts to waive remaining quality
checks through `.yydra/check-exceptions.toml`. It is distinct from the removed
vulnerability-exception records.

Ordinary dependency resolution and metadata used by remaining checks also stay.
For example, `cargo metadata` used for architecture or locked-workspace validation
is not the dedicated supply-chain inventory generator being removed.

The Android config plugin currently named `yydra-android-supply-chain` is ordinary
build configuration: it pins Gson `2.14.0` and commons-io `2.22.0`, and substitutes
the old Bolts coordinate with the retained local source module. Rename it and
its references to describe dependency configuration while keeping those behaviors.
Do not revert dependencies or remove the local module because of the old name.

Move the exact Bolts source-file attribution record out of the supply-chain-only
directory before removing that directory. Preserve original attribution data;
do not replace the exact source list with a directory-wide SPDX exemption or
introduce a new upstream source-review gate.

The existing server, H5 and Android nodes already retain their ordinary outputs;
the generic manifest hashes logs/artifacts and binds the executor. No substitute
supply-chain node or new generic artifact gate is required. Local check evidence
need not duplicate the CLI executable once the supply-chain copy disappears;
packaged acceptance records the actual package/executor identity, and CI retains
its one exact executor. This does not relax uploaded-evidence verification.

Removed checks must disappear from the current required/pass sets, not return
`pass`, an exception, or a successful no-op. Current contract documentation and
machine-readable claim boundaries must explicitly exclude supply-chain evaluation.
An explicit `--node supply-chain.*` request follows the unknown-node failure path.
Do not accept old catalogs/results as current evidence or synthesize missing
results. A changed template/catalog requires a new exact Distribution identity;
allocate and synchronize that identity during implementation without republishing
or mutating Distribution `0.1.0`.

## Implementation evidence and edit seams

These are inspected locations, not a prescribed line-by-line patch:

- [check_graph.rs](https://github.com/yydcnjjw/yydra/blob/d8d69ebc56e3959d73bf7e0d8d16d1b90f2ddf24/crates/yydra-cli/src/check_graph.rs): four node entries,
  dispatch and wrappers; Android task/material coupling; generic artifact and
  aggregate verification. Remaining nodes do not depend on the four removed nodes.
- [main.rs](../../crates/yydra-cli/src/main.rs): module import, inventory lifecycle,
  global snapshot validation, locked npm setup and exact-version enforcement.
- [former supply_chain.rs](https://github.com/yydcnjjw/yydra/blob/6b916aa870ddfefd66bd75af803e5c02a9634a7c/crates/yydra-cli/src/supply_chain.rs),
  [former supply-chain data](https://github.com/yydcnjjw/yydra/tree/6b916aa870ddfefd66bd75af803e5c02a9634a7c/crates/yydra-cli/supply-chain),
  [former generator](https://github.com/yydcnjjw/yydra/blob/6b916aa870ddfefd66bd75af803e5c02a9634a7c/scripts/generate-cli-supply-chain.mjs), CLI build script and
  [package manifest](../../crates/yydra-cli/Cargo.toml): remove exclusive consumers
  after preserving attribution. `flate2` and the build-target injection are
  supply-chain-only here; `semver` and `spdx` still have ordinary CLI consumers.
- [Product Workspace template](../../crates/yydra-cli/template/product-workspace):
  configuration files, Android plugin and declaration/tests, npm configuration,
  H5 runner/export settings, and active README/Baseline Skill instructions.
- [CLI README](../../crates/yydra-cli/README.md) and
  [quality workflow](../../.github/workflows/quality.yml): describe/call the revised
  graph, preserve full fixture/upload/aggregate paths, and disable implicit audit.
  CI currently has no independent supply-chain job.

At the inspected base, `setup` and `frontend.lock` used `npm ci` without disabling
audit; the template npm configuration only pinned its registry. The installed npm `12.0.2`
configuration and source confirm automatic audit is enabled by default. Suppress
it explicitly on Yydra-owned installation paths, including CI preparation; do
not confuse dependency download requests with advisory requests.

## Acceptance conditions

The conditions below define acceptance. The linked validation record contains
the observed results; source inspection alone does not count as running tests.

1. **Package and creation:** a newly packaged CLI is independently extracted and
   installed with its committed Cargo lock, then creates two identical-input
   clean Workspaces and an independent Reading Queue outside the repository.
   The two initial path/mode/byte inventories match. Package and Workspace contain
   no supply-chain-only config, scanner data, generator, or material-capture code;
   the retained attribution record and ordinary dependency plugin remain present.
2. **Supported preparation and integrity:** that installed CLI completes `doctor`,
   `setup`, `generate api`, and `generate api --check`. Both locks are preserved
   by setup/check. Missing/tampered ordinary authorities still fail read-only.
   Absence of removed supply-chain files no longer blocks creation or commands.
3. **Graph and reporting:** the catalog, required sets, summaries, diagnostic
   contract and aggregate omit the four removed nodes. Selecting a removed node
   fails as unknown. There is no removed-node pass/skip/waiver substituted for
   evaluation, and no affirmative vulnerability/SBOM claim. General deny-all
   quality exceptions and incomplete/focused-run semantics remain unchanged.
4. **No hidden scanning:** command-boundary tests verify no OSV/advisory request,
   automatic npm audit, Gradle material task, dependency-report task, SBOM or
   supply-chain-only dependency inventory generation. Successful H5/Android
   builds require no separate map/bundle capture or binding material. Ordinary
   dependency resolution, compiled application bundles and real builds still run.
5. **Dependencies and attribution:** fixed upgrades and Bolts substitution remain
   in generated Gradle configuration; keep plugin/idempotence/compatibility tests,
   complete LICENSE texts, original third-party headers, exact attribution
   classification, product source-license choices and DCO tests.
6. **Real remaining checks:** the same packaged executor completes fresh clean
   and Reading Queue graphs, including Rust/static/functional/database checks,
   Public API/Generated Client, real PostgreSQL/Axum/H5 behavior and registered H5
   semantics, repeated deterministic Android generation and actual release APK
   builds. No ignored test is counted as passed. Fake runners and leaf builds are
   useful regressions, not replacements for this packaged-consumer path.
7. **Aggregation and negative evidence:** aggregate the uploaded/retained complete
   clean and Reading Queue trees with that exact executor. Missing/altered logs
   or artifacts, stale/foreign catalog/executor/Distribution, failed/skipped/not-run
   remaining nodes and incompatible old evidence still fail. Wrong-version old
   Workspaces retain their existing rejection behavior.
8. **Repository and history:** run appropriate CLI/creation/check/attribution/package
   regressions and the remaining full suite, format/static checks and diff check.
   Current instructions match actual behavior; historical issues, ADR/release
   records and previous evidence retain their bytes and claims. Preserve unrelated
   WIP. Preflight disk for heavy builds, run local Android builds serially and
   reuse valid caches without treating cached evidence as a fresh acceptance pass.

Existing regression seams are
[consumer_creation.rs](../../crates/yydra-cli/tests/consumer_creation.rs),
[check_contract.rs](https://github.com/yydcnjjw/yydra/blob/d8d69ebc56e3959d73bf7e0d8d16d1b90f2ddf24/crates/yydra-cli/tests/check_contract.rs),
[licensing_policy.rs](../../crates/yydra-cli/tests/licensing_policy.rs),
[packaged_consumer.rs](../../crates/yydra-cli/tests/packaged_consumer.rs), and the
template's frontend/plugin tests. Remove only obsolete supply-chain cases from
mixed test files; retain authority-tampering, lock mutation, API drift, genuine
H5 failure, CNG nondeterminism, Gradle failure, missing APK, and evidence-tampering
negative cases.

## Subsequent workflow

The selected workflow is **`/implement` in one implementation session** using this
accepted scope. The work is one coherent subtraction within
the existing context and CLI/template acceptance path; a four-node subgraph is
removed with limited build/attribution decoupling. Long Android builds require
time and disk, not a new architecture or mandatory multi-session decomposition.

The selected scope requires no `/to-spec` or `/to-tickets` decomposition. Old
Workspace migration is outside this decision; adding it later would require a
separate scope and compatibility design. The original documentation-only session did not start implementation or invoke
tracker-publishing workflows. The subsequent `/implement` invocation is the
authority for local implementation and commit. Publication of
a future Distribution is also outside this implementation acceptance scope.

## Subsequent documentation storage

[ADR 0007](0007-store-research-and-validation-in-wiki.md), accepted on
2026-09-10, moves the historical research and validation collection to the
Wiki while retaining its original claims and code-repository history. It
changes the storage location, not the outcomes or raw evidence described
by this decision.

## Subsequent diagnostic and validation scope

[ADR 0009](0009-consolidate-diagnostics-in-doctor.md) retires `yydra check`,
the quality graph, and aggregate evidence. Current environment diagnostics use
`doctor`; project validation uses explicit Cargo/npm tests and builds. The
original decision and dated validation above retain their historical scope.
