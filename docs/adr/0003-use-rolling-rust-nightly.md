<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Use the rolling Rust nightly channel

Status: accepted — 2026-09-09.

The maintainer confirmed the consolidated scope and authorized implementation
with “确认共同理解并开始实施”. Implemented in Distribution 0.5.0 and locally
verified on 2026-09-09; see the [acceptance record](https://github.com/yydcnjjw/yydra/wiki/Validation-2026-09-09-Rust-Nightly/6a878e624ef5fba3d3069e3daf38a7169d798982).

The maintainer selected the rolling `nightly` channel with “滚动通道”, choosing
channel updates over a date-pinned nightly release. The selected toolchain
setting is `channel = "nightly"`. The maintainer subsequently selected
“只切换默认工具链（推荐）” and
“根仓库、CI、新 Workspace 及配套校验（推荐）”: change the default toolchain
across repository builds, CI, newly created Product Workspaces, and their
associated validation. Enabling a particular unstable feature or `-Z` option
is outside this selected change.

This decision amends the exact Rust toolchain constraint retained by
[ADR 0001](0001-remove-supply-chain-from-current-workflow.md) for the affected
Distribution. The implementation covers CI toolchain setup, runtime version
checks, required/observed tool evidence, packaged CLI installation instructions,
and toolchain-authority regression cases as well as the toolchain files.

## Selected validation and evidence policy

The maintainer selected “记录各自版本，允许汇总（推荐）”. Validate the rolling
nightly channel and required Rust components, and retain the actual observed
rustc, Cargo, rustfmt, and Clippy versions in each run's evidence. Preserve
reported commit/build information rather than replacing observations with
hard-coded version numbers or the word `nightly`.

Successful clean and Reading Queue runs using different nightly releases may
be aggregated. Each result remains bounded to the tool versions actually
observed; an aggregate does not establish compatibility with future nightly
releases or a common compiler build across its inputs. Exact Distribution,
executor, catalog, authored-input, and evidence-integrity checks retain their
existing roles. Other tool and dependency constraints are unchanged.

This is a subsequent amendment to ADR 0001's exact Rust toolchain requirement
for the affected Distribution. Preserve that ADR and historical evidence as
records of their original contracts.

## Implementation and acceptance boundary

Use `channel = "nightly"` in both repository and Product Workspace toolchain
files, retaining the minimal profile and rustfmt/Clippy components. CI installs
the rolling channel with the required components. Local channel refresh uses
the ordinary `rustup update nightly` workflow; builds and checks do not add an
implicit toolchain-update step.

The maintainer selected “仅维护 nightly，删除 MSRV 声明（推荐）”. Remove
`rust-version = "1.97.1"` from repository and template workspace manifests
together with their member crates' `rust-version.workspace = true` entries.
Document support in terms of the nightly versions actually validated; the
affected Distribution no longer declares or maintains a 1.97.1 minimum
supported compiler contract.
[Cargo's optional minimum-version field](https://doc.rust-lang.org/cargo/reference/rust-version.html)
declares package support and can influence dependency selection and tooling;
it cannot represent the nightly channel. Removing the declaration does not
establish compatibility with arbitrary older compilers.

Synchronize channel validation and actual-version recording through check
execution and aggregation, alongside the CI installation points, packaged CLI
installation instructions, current repair guidance, and relevant regressions.
Keep rejection of an altered template toolchain file, an effective stable
compiler override, missing required tools or evidence, and failed checks.
Permit different valid nightly versions in aggregate inputs while retaining
each input's exact observations. The existing string-map evidence fields and
raw version-command logs can express this policy without a new evidence schema.
Retain complete observed values rather than overwriting them with fixed short
versions.

Distribution 0.5.0 applies the template and contract change to newly created
Product Workspaces, following the repository's 0.4.0 API generation library and
build entrypoint implementation. Existing Product Workspaces and historical
releases retain their original contracts.

Acceptance includes repository formatting, Clippy and Rust regressions; real
packaged-CLI installation and fresh Workspace creation; version-policy and
aggregation cases covering distinct nightly builds and invalid observations;
and complete clean/Reading Queue checks with real Rust, API generation, H5 and
Android build paths. Record the actual nightly used and any failed or unrun
checks. Preflight disk space for heavy builds and reuse valid build caches.
The [acceptance record](https://github.com/yydcnjjw/yydra/wiki/Validation-2026-09-09-Rust-Nightly/6a878e624ef5fba3d3069e3daf38a7169d798982) records passing
repository regressions, both complete 29-node checks, and aggregation from
retained evidence using the independently packaged 0.5.0 executor. These local
results apply to the observed nightly and do not establish compatibility with
future nightly releases.

## Subsequent documentation storage

[ADR 0007](0007-store-research-and-validation-in-wiki.md), accepted on
2026-09-10, moves the historical research and validation collection to the
Wiki while retaining its original claims and code-repository history. It
changes the storage location, not the outcomes or raw evidence described
by this decision.
