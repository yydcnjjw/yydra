<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Rolling Rust nightly acceptance

Candidate Distribution: `0.5.0`. Scope: [ADR 0003](../adr/0003-use-rolling-rust-nightly.md).
Status: passed locally on 2026-09-09; candidate packages have not been published.
Packaging baseline commit: `0f8ad10a2f5b00286e6a2533eaa86a7f6b3532e2`.
Final worktree HEAD: `c68d49bc983fa34454a90c99f6fad3d2674414ce`.
The intervening commits changed repository guidance and documentation only;
the packaged source comparison below confirms the validated inputs still match.

## Candidate identity

The helper and CLI were independently packaged with nightly Cargo using
`--locked --offline --allow-dirty`, including Cargo's package verification.
The CLI was installed from its extracted package with its packaged lock.
Its bundled helper files were byte-compared with the independently packaged
helper; the bundled manifest matches `Cargo.toml.orig`. All ten inspected
package/template manifests omit the stable MSRV declaration.
After acceptance, 111 packaged source, template, manifest-source, and support
files were byte-compared against the working tree with no mismatches.

| Artifact | Bytes | SHA-256 |
| --- | ---: | --- |
| `yydra-build-0.5.0.crate` | 13958 | `12b0571b7ee5f53ff2bd94ef67f1f359a2c7f9cdb8e8a230d6fd3f67131a392f` |
| `yydra-cli-0.5.0.crate` | 344637 | `e8aaaa4f4471eb55fabaf3d969a9fc79a7ef95e20d602a96c2439cdb34fccc60` |
| Installed `yydra` | 4392072 | `09aec69297943ad88980dd6495c730e4f1cafa91cefca4b3ebc06f17918dbdda` |

## Validation results

- Repository and fresh Workspace formatting passed. Repository Clippy with
  `--locked --workspace --all-targets -- -D warnings` passed.
- `cargo test --locked --workspace -- --test-threads=2` passed: 124 tests,
  zero failures, five explicitly ignored infrastructure-dependent tests.
  Ignored tests are not counted as passed.
- Regressions exercise actual version-output preservation, effective stable
  compiler rejection, distinct-nightly aggregate inputs, missing/malformed
  observations, and the existing artifact/identity tampering cases.
- Independently installed CLI creation, `doctor`, and setup passed. Its default
  build produced the release server executable and H5 production output.
- The packaged clean and Reading Queue Workspaces each passed all 29 required
  checks, including real Android APK builds, database invariants, H5
  accessibility/runtime, and authored-input immutability. The harness also
  compared each Workspace's authored source and lockfiles with its creation
  inventory; both were unchanged.
- Each complete evidence tree was copied without disposable scratch output.
  The exact packaged executor aggregated those retained manifests, JSONL,
  logs, and artifacts successfully: `complete = true`,
  `aggregateConformance = true`, `status = "pass-aggregate"`.

| Evidence manifest | Result | SHA-256 |
| --- | --- | --- |
| `retained/clean/manifest.json` | `pass-core`, 29/29 | `ef2ab2748cf059145ba049a0857c20776cab6a8c339c0866ebdebae041915619` |
| `retained/reading-queue/manifest.json` | `pass-core`, 29/29 | `e4696f1b3209078e27afef7c82aee3430d84b5917f81c9b86b2ab6d2853a5d47` |
| `aggregate-evidence/manifest.json` | `pass-aggregate` | `b74b0eb2b4f4039e5ff2bf7cc774f1a461f1ea3727835bd960fab2ddb49d93b0` |

Each fixture retains its APK at `artifacts/android.release/app-release.apk`:

| Fixture | APK bytes | SHA-256 |
| --- | ---: | --- |
| clean | 96927296 | `54073ee6ca542f92139f7b2b7a9f85cf920cec3dab41fa393bc65655ec84c3c0` |
| Reading Queue | 96927296 | `79fe24ace4eec179822997fcee665d767f27d261abca1dfe1c0003b9c8cbc9a3` |

## Tool and evidence boundary

Both real runs used the same installed nightly, with the following complete
observations retained separately in each manifest. The cross-date aggregate
regression uses synthetic tool observations; it does not establish that
another compiler release builds this product. Future nightlies remain subject
to the actual checks.

```text
rustc 1.100.0-nightly (cea272fa3 2026-09-07)
cargo 1.100.0-nightly (3c0b53475 2026-09-04)
rustfmt 1.10.0-nightly (cea272fa35 2026-09-07)
clippy 0.1.100 (cea272fa35 2026-09-07)
```

Acceptance ran locally on Linux/WSL2. GitHub Actions configuration was updated
but no remote workflow was triggered. The runtime evidence covers H5 and local
Android release builds without Expo/EAS accounts; it does not cover Android
device execution or store distribution. The five ignored Rust tests remain
unrun as individual tests; the separate complete graphs exercised the real
positive runtime paths.

Local source diff, tool identities, package checksum files, full-suite logs and
summary, and the acceptance harness are retained under
`/home/yydcnjjw/.cache/yydra-nightly-20260909-ghgjoskk/`. Runtime receipts and evidence are under
`candidate-locked/` there. The harness reuses the verified Gradle wrapper and
existing dependency seed, and compares created source and lockfile hashes
through the actual build/check path. Existing historical evidence is preserved.
