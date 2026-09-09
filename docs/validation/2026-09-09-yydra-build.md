<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# yydra-build and application build acceptance

Candidate Distribution: `0.4.0`. Baseline commit:
`6ce8ed482104c85a8f1239635d8774205239931e`.
Scope: [ADR 0004](../adr/0004-provide-yydra-build.md). The rolling-nightly
proposal remains outside this change. No registry publication or release was
performed.

The helper and CLI are packaged independently with `cargo package --locked
--offline --allow-dirty`, including Cargo's package verification. The CLI is
installed from its extracted package using its packaged lock. Its bundled
helper source, manifest, README, and complete license texts are byte-compared
with the independently packaged helper; template symlinks flatten into regular
files. A newly created Product Workspace supplies the acceptance input.

## Candidate identity

| Artifact | Bytes | SHA-256 |
| --- | ---: | --- |
| `yydra-build-0.4.0.crate` | 13973 | `b4e95012ed6fa47acd17dd18e442f7dd858ab86cd51d2f7c24793fb7a6978416` |
| `yydra-cli-0.4.0.crate` | 342702 | `b36cdf0f19d75cc0127b2d229e5771b836f0966ecc60f00f1d604c2bfc218040` |
| Installed `yydra` | 4383520 | `7656e44894a0c5d551cce08235db79bee326bb51e1485763b504fa77b71fafd8` |

The final CLI package differs from the runtime candidate only in
`tests/packaged_consumer.rs`, which now prepares frontend tools before compiling
the complete generated Workspace. Repackaging and reinstalling produced the
same executable bytes shown above. All production code and template files are
identical. `candidate-locked/final-package/receipt.json` records this comparison;
`candidate-locked/runtime-archives/` retains the runtime candidate archives.
The runtime receipt retains the original package staging paths; use
`runtime-archives/` for those original artifact bytes after repackaging.
Its CLI archive is 342611 bytes with SHA-256
`63aedbd0c50102e42a5f93d1b372ae14f7d697cc17486f08b4794dfa94b69230`.

## Verification

- Repository tests: 121 passed, five explicitly ignored; doc tests passed with
  zero cases. The full suite passed its first 120 tests, then exposed the old
  packaged-consumer fixture ordering. Adding `setup` before the unchanged
  whole-Workspace Cargo test made that final test pass on its focused rerun.
  Both the initial failure and corrected result are retained.
- `cargo clippy --workspace --all-targets --locked -- -D warnings` and formatting
  checks passed.
- The packaged CLI created and diagnosed a fresh Product Workspace. Targeted
  server and migration compilation passed before installing frontend tools.
  Default release backend/H5 and explicit H5 builds passed with real Orval and
  TypeScript. All 45 frontend tests, formatting, and lint passed.
- Unchanged generation reused output; removing the link restored it without
  regeneration; removing a generated client file rebuilt it. An intentional
  Orval failure stopped the consumer, and repairing the input restored success.
  A custom target directory containing spaces retained an unrelated cache probe.
  Authored source and both dependency locks remained byte-identical.
- Explicit `yydra build --target android` produced a real account-free release
  APK (96927296 bytes; SHA-256
  `2d3ff121eb6e324a618c169670f8948ce35e7c96fd8ae2b99c168eec8e5cbe76`).
  The artifact is preserved as `candidate-locked/artifacts/public-build-app-release.apk`.
- The complete `yydra check --fixture clean` passed all 29 nodes with
  `complete: true` and `status: pass-core`. This includes an independent Android
  release build, server release, API client/runtime contracts, database and
  post-commit executor invariants, H5 accessibility and real-runtime checks,
  and authored-input immutability. The manifest SHA-256 is
  `87177ceb2e943322e3977e3531fb66bc16c483fb731d47117da1bbb3eeb39d8e`.
  Acceptance finished at `2026-09-09T04:08:47Z`.

The public build tests cover default backend/H5 selection, server-only
selection, consecutive account-free Android builds, propagation of frontend
failures despite stale artifacts, and rejection of `generate api`. Real Cargo
integration tests cover generation without full Workspace identity checks,
unchanged-input reuse, changed-input generation, missing-output recovery,
frontend relinking, invalid contracts/tools/clients, failure and repair, custom
target directories, and sequential shared caches. Doctor tests reject modified
and additional helper source. The check catalog tests require prerequisites to
precede their consumers.

Two review findings were reproduced with failing tests and fixed: repeated
Android builds collided with previous fixed log files, and snapshot checks
accepted an extra helper build script. Standards and Spec re-review each found
zero unresolved issues. Earlier candidate/suite runs were intentionally
interrupted to apply those fixes; they are not counted as final acceptance.
A subsequent packaged run detected Cargo.lock ordering changes because recovery
used `cargo clean` without `--locked`. The final command adds `--locked`, and
an additional failing/passing regression verifies byte preservation with a
valid lock whose package order differs from Cargo's serialization order. The
failed candidate receipt is retained. A harness-only relinking assertion also
switched npm to direct Node, changing tracked PATH; final verification uses the
same npm entrypoint to test unchanged-input reuse.

Evidence is retained locally beneath
`/home/yydcnjjw/.cache/yydra-build-20260909/`. The final packaged run uses
`candidate-locked/receipt.json`; complete quality evidence is in
`candidate-locked/clean-evidence/`; repository test output is `full-suite.log`,
with the corrected packaged test in `packaged-consumer-after-setup.log`.
Public Gradle wrapper and dependency caches are reused with verified wrapper
copying. Build outputs and frontend links are disposable; authored source and
both dependency locks are compared before and after acceptance.

This is local Linux clean-fixture core conformance. It does not establish
cross-fixture release aggregation, device UI acceptance, or store publication
and signing.
