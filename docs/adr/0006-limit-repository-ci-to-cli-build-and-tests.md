<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Limit repository CI to CLI build and tests

Status: accepted — 2026-09-10. The maintainer confirmed the consolidated scope
and authorized local implementation. Remote required-check migration and PR
integration remain separate from local completion.

Repository CI previously compiled a Distribution executor, ran the complete
Mechanical Quality Contract for clean and Reading Queue Product Workspaces,
and aggregated their Conformance Evidence. The maintainer selected a narrower
current scope: “目前只需要包含 yydra-cli 的构建 以及 测试 的 ci 就够了”.
Repository CI should build and test `yydra-cli`; it should no longer run the
two complete Product Workspace checks or their aggregate as its CI workload.

This changes the evidence required from routine repository CI. A successful CLI
build and test run supports a claim about the exercised CLI behavior; it does
not establish complete Product Workspace conformance. The existing Product
Workspace Mechanical Quality Contract and release-validation requirements are
outside this selected change.

The selected scope amends the complete-CI requirement in the
[GitHub development workflow](../agents/github-workflow.md), which previously
required both complete fixture jobs and their aggregate even for documentation
PRs. Workflow changes, merge guidance, and the required status-check names must
be coordinated. Retain the independent DCO workflow and its required
`Every submitted commit is signed off` check.

## Selected test boundary

Run CLI unit tests and command-behavior regressions using controlled test
inputs and substitute external tools where the existing tests provide them.
Keep CLI integration tests that meet this boundary; selecting only the binary's
unit tests would omit meaningful command-behavior coverage.

Retain tests that execute real consumers as explicitly invoked integration
tests outside this CI workload. This includes real consumer compilation,
frontend setup and checks, API generation through Node and a consumer build,
packaged installation acceptance, and real database, browser, or native-build
acceptance. Classify the complete execution path: replacing one external tool
with a fake does not make a test independent of real tools used by its
prerequisites. Extend the existing descriptive `#[ignore = "requires …"]`
convention so the default test run reports these exclusions and they remain
available through explicit test selection with `--ignored` or
`--include-ignored`. Preserve their assertions and document required tools.

The accepted trade-off is that routine CI provides narrower integration
coverage. It no longer proves that the generated applications build and pass
their complete checks. Local validation still follows the changed behavior,
and release candidates still need the existing complete validation.

## Implementation

Replace the complete quality workflow with `Yydra CLI CI`, using one
`Build and test yydra-cli` job on `ubuntu-24.04` and the rolling Rust nightly
channel. Retain the existing PR, push-to-main, and manual triggers, including
documentation PRs. Run these commands sequentially after preparing Rust:

```console
cargo build --locked --release --package yydra-cli
cargo test --locked --package yydra-cli --all-targets
```

Remove the two fixture jobs, aggregate job, and their tool setup and evidence
uploads from repository CI. The CLI build is used to verify compilation;
this workflow does not publish a Distribution. DCO remains its own workflow.

Update repository agent/contributor guidance and the existing workflow
regression to describe the narrower CI claim. Preserve historical decisions
and validation records; add current amendment pointers where needed. Do not
change the Product Workspace check graph or its outcome/evidence semantics.

For GitHub integration, replace the four old quality-job requirements with
`Build and test yydra-cli`, retaining DCO and all other existing ruleset
settings. Coordinate that replacement with the reviewed workflow candidate
and verify the actual new job and DCO results before merge. An old missing
required job must not be left blocking the new workflow; a skipped or missing
new job must not count as success. Prepare and validate the local change first;
remote rule updates and PR integration require scoped authorization and a
read-back of the effective configuration.

Validate the release CLI build, the complete default CLI test selection, and
the workflow and documentation changes. Review every newly excluded test and
retain its explicit invocation path. Changes limited to CI and test selection
do not alter the Android build chain and do not require an Android build under
the local development workflow. The local validation result and remote
integration status are recorded separately when the corresponding work finishes.

## Local validation and integration status

Implemented and locally verified on 2026-09-10. The CLI release build, Rust
format check, Clippy with warnings denied, workflow YAML parsing, and changed
Markdown link checks passed. The default CLI suite passed 84 tests with zero
failures and 44 explicitly ignored consumer integration tests. A temporary
validation PATH rejected real Node/npm, Docker, Java/Gradle, and consumer Cargo
resolution/build commands; the passing default suite made no such calls.
Consumer test bodies and assertions were preserved; only their selection
annotations and the obsolete workflow regression changed.

Validation used `rustc 1.100.0-nightly (cea272fa3 2026-09-07)` and
`cargo 1.100.0-nightly (3c0b53475 2026-09-04)`. Android and the retained consumer
integration tests were not run under this CI/test-selection-only change.
The GitHub ruleset still requires its previous checks at this local handoff;
no push, PR, ruleset update, or merge has been performed for this task.

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
