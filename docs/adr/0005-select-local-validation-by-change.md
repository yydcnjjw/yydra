<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Select local development validation by change

Status: accepted — 2026-09-09. The local workflow is active through the root
`AGENTS.md` entry.

Yydra repository development should select local validation according to the
change. Require a real Android release build when changing Android native
dependencies, configuration, or its build chain, and retain complete validation
before a Distribution release. This reduces routine development waiting while
accepting that Android integration failures outside those triggers may be found
at a later full validation.

The scope is the local workflow used by agents developing this repository.
Existing CI and the Product Workspace Mechanical Quality Contract remain in
force. This decision records when local development invokes those checks; it
does not change their implementation, required nodes, or evidence semantics.

The existing repeated `yydra check --node` arguments select checks and their
prerequisites. A successful selected run reports `pass-selected` with
`complete: false`; it is not complete or aggregate Conformance Evidence.
Default `yydra build` already selects backend and H5, with Android selected
explicitly.

Ordinary Rust backend, API definition, and shared React or TypeScript business
changes may omit Android when native configuration, dependencies, and build
tooling are unchanged. API generator, Metro or packaging configuration, native
code, and Android build or check implementation changes require Android
validation. Classify the behavior changed, including affected dependencies,
rather than treating every edit to a shared file as an Android change.

The [local development workflow](../agents/development-workflow.md)
contains the concrete validation selection, repeat-run, completion, and release
rules referenced by the root `AGENTS.md`. No CLI, template,
CI, Gradle, or cache implementation change is part of this decision.

## Decision evidence

The [Distribution 0.4.0 validation record](https://github.com/yydcnjjw/yydra/wiki/Validation-2026-09-09-Yydra-Build/6a878e624ef5fba3d3069e3daf38a7169d798982)
identifies the retained local manifest. In that run, `android.release` took
2,178,409 ms out of 2,554,302 ms summed across all check nodes (85.3%). Other
nodes summed to 375,893 ms. These are observations from one completed run,
not a future latency guarantee or the total development-session duration.

## Subsequent CI amendment

[ADR 0006](0006-limit-repository-ci-to-cli-build-and-tests.md), accepted on
2026-09-10, separately narrows repository CI to CLI build/tests plus DCO.
The original decision above retained the then-current CI; its local validation
selection and complete release-validation requirements continue to apply.
