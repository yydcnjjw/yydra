<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Local development workflow

Status: active — 2026-09-10.
Decision: [ADR 0005](../adr/0005-select-local-validation-by-change.md).

This workflow applies to agents developing the Yydra repository locally.
It selects validation for a development task. Repository CI separately builds
and tests the CLI under [ADR 0006](../adr/0006-limit-repository-ci-to-cli-build-and-tests.md).
The Product Workspace Mechanical Quality Contract retains its requirements.
An explicitly agreed task-specific acceptance condition still applies; this
default does not silently amend an existing task or ADR requiring full validation.

Use the [worktree development workflow](worktree-workflow.md) for task
isolation, creation, synchronization, local commits, handoff, and authorized
cleanup. Run the checks selected here against that task's candidate.

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
`check_graph.rs` does not automatically affect Android, and a frontend lockfile
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
complete Rust collection replaces candidate-specific aggregate release evidence.

## Use the existing focused check entrypoint

Use a CLI built from the relevant candidate, with the matching Product
Workspace. Select all applicable nodes in one invocation so shared
prerequisites execute once. The paths below are illustrative external consumer
Workspace paths, not this repository's root.

For example, API and frontend changes can use the following selected checks;
add affected runtime, database, H5, or repository tests as the change requires:

```console
yydra check /path/to/consumer \
  --node api.runtime-conformance \
  --node api.client-contract \
  --node frontend.test
```

When Android is triggered, select `android.release`; its prerequisites already
include repeated native generation and API generation:

```console
yydra check /path/to/consumer --node android.release
```

Additional affected nodes can join that invocation. Default `yydra build`
produces backend/H5 artifacts. It is useful when those artifacts are needed;
it does not replace the selected tests or establish full conformance.

## Avoid repeated expensive validation

Run faster relevant repository checks before starting an Android release build. For a
given final set of relevant inputs, use one required Android validation run.
Do not routinely follow a successful `android.release` check with a separate
`yydra build --target android` or a full `yydra check`. Testing a changed public
build entrypoint or an explicitly required repeated-build scenario can justify
additional runs; explain that purpose.

After a successful run, repeat it only when relevant inputs, executor, or
configuration changed, a failure needs verification after repair, or a new
finding invalidates its coverage. Record which run supports the current
result. Local reuse does not convert selected evidence into release evidence.
Keep valid caches and preflight disk space for heavy builds; follow the
existing supported cache and resource limits.

## Finish a development task and validate a release

A local task can finish when its agreed change is implemented and reviewed,
the checks required for that change have passed, and its local cryptographically
signed commits carry author-matching DCO sign-offs under the worktree workflow.
Report what ran, its result, and material coverage limits. When Android was not
triggered, say that it was not run under the change scope. A selected check result remains
`pass-selected`, `complete: false`; do not describe it as full or aggregate
Conformance Evidence or relabel an unrun node as passing.

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

Before publishing a Distribution, require the existing complete clean and
Reading Queue validation and aggregate evidence for the release candidate,
including Android. A local version bump or package test is not by itself a
publication step. CLI CI does not supply this complete release evidence.
Respect any stronger explicit acceptance conditions of the current task.
Validation does not authorize publication.
