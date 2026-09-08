<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Supply-chain removal validation

Status: passed — local implementation acceptance, 2026-09-08.

This record implements the accepted
[scope amendment](../adr/0001-remove-supply-chain-from-current-workflow.md).
The local Distribution candidate is `0.2.0`; its result schema is `2` and its
Mechanical Quality Contract contains 29 required nodes. The four removed nodes
are absent, and dependency inventories, advisories, vulnerability exceptions,
SBOMs, and dependency-material attribution are explicitly `notEvaluated`.

## Candidate identity

The candidate was packaged from the implementation working tree based on
`6b916aa870ddfefd66bd75af803e5c02a9634a7c`, before the local implementation commit.
These hashes identify the actual package and independently installed executor
used below. They are local acceptance artifacts, not published release assets.

| Artifact | Bytes | SHA-256 |
| --- | ---: | --- |
| `yydra-cli-0.2.0.crate` | 350242 | `cbca92f61e1bc327e62cfb7c91112cec742b38c5763a0bfd1c806c54ed3fffd6` |
| Installed `yydra` | 4731272 | `1aa14da311b0b59662dfd09f83cbcbdc0a6d8c2bdb76d7ac3021b8ca3171ab3d` |

The package was verified by `cargo package --locked --offline --allow-dirty`,
extracted outside the repository, and installed with `cargo install
yydra-cli@0.2.0 --path <extracted-package> --locked --offline`. Existing compiler
caches were reused. Its 132-file inventory contains the retained Bolts
attribution record and Android dependency plugin, with no supply-chain scanner,
dedicated data directory, build script, or Workspace supply-chain configuration.

## Repository regressions

`cargo test --locked --workspace --all-targets --all-features --
--test-threads=1` completed with **119 passed, 0 failed, 5 ignored**.
The ignored integration cases remain ignored in this count; the actual packaged
checks below provide separate runtime and build evidence.

`cargo check --locked --workspace --all-targets --all-features`,
`cargo clippy --locked --workspace --all-targets --all-features -- -D warnings`,
`cargo fmt --all -- --check`, and `git diff --check` passed.

Discriminating regressions cover absent supply-chain configuration, unknown
removed nodes, disabled automatic npm audit, H5 exports without forced maps,
APK-only Android evidence, real build failures, required-test removal, authority
and lock mutation, and missing/tampered/stale aggregate evidence. The existing
Bolts compatibility, exact attribution and dependency-upgrade checks remain.

The retained `tests/fixtures/BoltsSourceContract.java` also passed against the
actual package's 13 original Bolts Java sources, compiled with Java 21
`javac --release 8` and the installed Android 36 API JAR. This covers task
continuation, error and cancellation/close behavior on the JVM. The diagnostic
fixture is repository-only; it was not added to the consumer package. This is
separate from Android runtime behavior.

## Packaged Product Workspaces

The installed executor created two `Clean Product` / `clean-product` Workspaces
and an independent `Reading Queue` / `reading-queue`, each declaring
`Apache-2.0` product source licensing. Both initial clean path/mode/byte
inventories matched, and all three lacked the removed configurations.

All three completed `doctor`, `setup`, `generate api`, and `generate api --check`
through that executor, preserving both Cargo and npm lockfiles.

The clean Workspace completed **29/29** required nodes, including the real
PostgreSQL/Axum/H5 paths and Android release assembly. Gradle reported
`BUILD SUCCESSFUL in 37m 58s` with 651 executed tasks. Its APK is 96927296 bytes,
SHA-256 `bea29987369e9da68ef8a7dfe7e8c57fa1dded646d50959b3a791782c371d6cc`.
The complete manifest's SHA-256 is
`05f12f048668a9e1a45ff26b82437bd0a49a5ba80eb46d48561bcaa66e50eadb`.

An independent comparison confirmed that all 169 initial path/mode/byte
inventory entries in each of the three original Workspaces remain unchanged.
The successful clean run removed its scratch tree while retaining normal
outputs and logs.

Reading Queue also completed **29/29** required nodes, including the real H5
paths and Android release assembly. Gradle reported `BUILD SUCCESSFUL in 36m 59s`
with 651 executed tasks. Its APK is 96927296 bytes, SHA-256
`62c7abc816a62cdacb9437b42cc681dd5702aacfbc29c656394a0525ea1197f6`.
Its complete manifest's SHA-256 is
`998b4744da30f239603d08addaeb8fa3963f634e1a143c70522ee10c2b52c023`.

Both successful evidence trees were copied to separate retained locations and
verified by the same packaged executor. Aggregation returned `pass-aggregate`,
`complete: true`, and `aggregateConformance: true`. The aggregate manifest's
SHA-256 is `e9cfae3f485d62c47fa032ce94cdeb5b5ae94f67911f6337ab4593e5f43ba072`.
The final independent comparison again confirmed all 169 original inventory
entries per Workspace remained unchanged, including both locks. Both successful
scratch trees were removed, and the retained manifests match their originals.

Both complete manifests use schema `2` and contain only the 29 remaining nodes,
all passed. Command and artifact checks confirmed no Gradle dependency-report or
material task, no forced separate Android bundle/map, and no dedicated material
files. The same commands explicitly disable automatic npm audit. Both ordinary
H5 and APK production paths succeeded without supply-chain material prerequisites.

These results establish the accepted local scope on Linux x86_64. They do not add
Android runtime, physical-device, native-accessibility, iOS/macOS or supply-chain
claims. `clean-b` establishes deterministic creation and supported preparation;
the two complete fixture graphs are `clean-a` and `reading-queue`.

## Standards

Independent review found no unresolved documented-standard violations or
actionable code smells after the Android dependency helpers were renamed to
match their retained behavior.

## Spec

Independent review found no unresolved deviation from the accepted scope.
Existing Workspaces still require their original exact Distribution. The
source replacement, exact attribution, ordinary dependency upgrades, general
quality-exception rejection, generated authority and build/evidence integrity
remain in scope.

## Evidence and preservation

Local logs and the acceptance controller are under
`/home/yydcnjjw/.cache/yydra-remove-supply-chain-20260908/`.
`packaged-acceptance/receipt.json` records command arguments, durations,
environment and artifact identities. Full raw evidence stays outside the
repository.

Validation uses a disk-backed temporary directory and serial Workspace/Android
runs. Cargo uses one job for the repository suite and two for packaged
acceptance. Android retains its existing worker bounds and the validated
`target/yydra-gradle-ro-cache-v2` dependency seed.

The original Bolts attribution JSON was moved byte-for-byte to
`crates/yydra-cli/third-party/bolts-source.json`. The old Bolts ADR, `0.1.0`
release procedure and unrelated working-tree content retain their original
bytes. No historical issue, release, package or validation record is rewritten.
