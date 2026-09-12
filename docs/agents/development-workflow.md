<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Local development workflow

Status: active — 2026-09-12.
Decisions: [ADR 0005](../adr/0005-select-local-validation-by-change.md) and
[ADR 0009](../adr/0009-consolidate-diagnostics-in-doctor.md).

This workflow applies to agents developing the Yydra repository locally.
It selects validation for a development task. Repository CI separately builds
and tests the CLI under [ADR 0006](../adr/0006-limit-repository-ci-to-cli-build-and-tests.md).
The retired quality graph and evidence protocol are replaced by explicit project
tests and builds. Doctor diagnoses Workspace identity and environments.
An explicitly agreed task-specific acceptance condition still applies; this
default does not silently amend an existing task or ADR requiring full validation.

Use the [worktree development workflow](worktree-workflow.md) for task
isolation, creation, synchronization, local commits, handoff, and authorized
cleanup. Run the checks selected here against that task's candidate.

Track validation outputs under the worktree workflow's
[task artifact rules](worktree-workflow.md#track-task-artifacts-and-caches).
Place controllable temporary outputs in `.task/tmp/` and register actual
external paths in `.task/artifacts.md`, including tool-created files under
`~/.cache` and `/tmp`. Preserve supported shared caches while distinguishing
them from disposable consumer Workspaces, build outputs, and task-private
caches. Record a reason and retention condition for evidence kept after the
task; cleanup follows the inventory even when those files are outside Git.

## Select validation from the change

Inspect the task, relevant domain documents, and current diff before selecting
checks. Preserve unrelated work. State the affected behavior, intended checks,
and whether Android is triggered in a brief progress update. Ordinary check
selection under this workflow does not need another confirmation round.

Run the relevant repository formatting, static checks, and regression tests.
Add consumer validation when the change affects generated Workspaces. Changes
to CLI packaging, creation, templates, or bundled build support need the
relevant packaged-CLI and fresh-Workspace checks; that requirement alone does
not mandate a full Android build.

| Changed behavior | Local validation | Android release build |
| --- | --- | --- |
| Documentation only | Review content, links, and diff formatting | No |
| Rust backend or CLI behavior outside the Android build chain | Relevant Rust checks and regressions; affected consumer checks | No |
| API definitions or shared React/TypeScript business behavior | Relevant API generation, contract, frontend, and H5 checks | No, when native dependencies/configuration and build tooling are unchanged |
| API generation tooling or its build integration | Generator regressions and relevant consumer/API/H5 checks | Yes |
| Native code/dependencies, Expo Android configuration/plugins, Metro or Android packaging configuration | Relevant regressions and native generation/build checks | Yes |
| Android build/check implementation or tooling used by that path | Relevant runner/tooling regressions and native generation/build checks | Yes |

Judge the actual change and affected dependency graph. An edit to
a diagnostic/reporting change does not automatically affect Android, and a frontend lockfile
edit must be inspected for changes to native dependencies or build tooling.
A Rust-only dependency or formatter change is not itself an Android trigger.

## Run repository CLI tests

The default CLI suite runs unit tests and command-behavior regressions:

```console
cargo test --locked --package yydra-cli --all-targets
```

Tests using real consumer dependencies, compilation, Node/npm, databases,
browsers, or native tooling carry descriptive `#[ignore = "requires …"]`
annotations (newly classified tests use the `consumer integration:` prefix).
They remain in the suite and must be explicitly selected when the change
requires that coverage. Inspect each annotation and test's setup; some fixtures
use offline Cargo operations and need their consumer dependencies fetched first.

For example, after preparing the required Node/npm versions and Cargo caches:

```console
cargo test --locked --package yydra-cli --test api_generation -- --ignored
cargo test --locked --package yydra-cli --test packaged_consumer -- --ignored
```

Use `--test <target> <test-name> -- --ignored` for a particular integration test.
`cargo test --locked --package yydra-cli --all-targets -- --include-ignored`
requests the complete Rust test collection, including real consumer acceptance;
it requires the corresponding tools, caches, Docker image, and browser setup.
An ignored test is unrun, not passing. Neither the default CLI suite nor the
complete Rust collection replaces the release candidate's actual validation records.

## Validate Product Workspaces explicitly

Use the candidate's packaged CLI and a matching fresh Product Workspace when
creation, templates, packaging, or bundled build support change. `yydra doctor`
reports environment and Workspace problems; it does not run validation.
Run `yydra setup` explicitly before consumers of frontend dependencies.

Select the affected commands from the Product Workspace:

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
yydra build . --target h5
```

Use a Cargo test target/filter or the frontend runner's test filter when it
covers the changed behavior. Database integration requires a fresh disposable
PostgreSQL database, `DATABASE_URL`, migration, and explicit `--ignored`;
run the `reading_queue_postgres` test target serially. H5 tests require a
running migrated backend, Playwright Chromium, and `EXPO_PUBLIC_API_URL`.
The generated README documents the commands and cleanup; never use deployment
data for destructive test fixtures. Existing isolated repository consumer
integration tests may also supply those prerequisites.

When Android is triggered, use `yydra build <consumer> --target android` and
relevant generation/build regressions. Preserve the actual APK and build log.
Where a representative architecture is selected, record it and its coverage
limit explicitly. The build checks generation input integrity and produces an
APK; it does not establish repeated-generation or cross-host reproducibility.

## Avoid repeated expensive validation

Run faster repository regressions first. For a final set of relevant inputs,
use one required Android build. Repeat only if inputs, executor, or configuration
change, a failure needs confirmation after repair, or another explicit
acceptance scenario requires it. Explain any additional build's purpose.
Preserve supported dependency caches and preflight disk/temporary space.

## Finish a development task and validate a release

A local task can finish when its agreed change is implemented and reviewed,
the checks required for that change have passed, and its local cryptographically
signed commits carry author-matching DCO sign-offs under the worktree workflow.
Report what ran, its result, and material coverage limits. When Android was not
triggered, say it was not run under the change scope. Keep each result bounded
to the actual tests, inputs, and platforms used; do not relabel an unrun test
as passing or describe diagnostic success as application validation.

Put new standalone validation records in the
[Yydra Wiki](https://github.com/yydcnjjw/yydra/wiki/Home), following
[ADR 0007](../adr/0007-store-research-and-validation-in-wiki.md). Task and PR
reports still summarize the checks performed; a separate Wiki page is not
required for every routine run. Detailed records identify the exact candidate
and executor, performed checks, outcomes, retained evidence locations, and
coverage limits. Preserve dated outcomes and mark later corrections separately.
An ADR relying on a record links to its fixed Wiki revision. Publishing a record
does not upload the artifacts it describes or establish new validation results.

The [GitHub development workflow](github-workflow.md) adds the separate PR merge
gate: DCO and CLI build/test CI must succeed before merging.
Local completion, including a documentation-only task that did not trigger
Android locally, does not waive that gate or authorize external actions.

Before publishing a Distribution, validate the candidate's packaged CLI and
fresh Product Workspaces through creation/setup/doctor, relevant Rust/API and
frontend tests, PostgreSQL integration, production H5 flows, and a real Android
build. Use disposable integration services and retain actual commands, versions,
logs, and artifacts needed by the report. The CLI no longer grants aggregate
conformance. A local version bump or passing CLI CI alone is not release
acceptance. Honor stronger explicit task acceptance and publication authorization.
