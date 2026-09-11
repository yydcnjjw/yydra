<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Context Map

## Contexts

- [Yydra Framework](./CONTEXT.md): distinguishes the application framework, its reusable capabilities and distributions, product-owned domain code, generated workspaces, and validation products

## Relationships

- Additional contexts and their relationships will be added when their boundaries are resolved.

## System-wide decisions

- [Pinned Bolts source replacement](./docs/adr/2026-09-05-pinned-bolts-source-replacement.md): historical replacement decision and its 2026-09-05 review-scope amendment
- [Remove supply-chain evaluation from the current workflow](./docs/adr/0001-remove-supply-chain-from-current-workflow.md): accepted subsequent scope amendment; preserves ordinary dependency, ownership, build, and Conformance Evidence guarantees; implemented and locally verified
- [Simplify API generation](./docs/adr/0002-simplify-api-generation.md): accepted; sequential generation, narrower entrypoint checks, disposable build outputs, and current-version-only validation; implemented in Distribution 0.3.0 and locally verified
- [Use the rolling Rust nightly channel](./docs/adr/0003-use-rolling-rust-nightly.md): accepted; nightly-only support for repository, CI, and new Workspaces; no stable MSRV declaration, actual per-run versions, and mixed-nightly aggregation; implemented in Distribution 0.5.0 and locally verified
- [Provide reusable API generation through yydra-build](./docs/adr/0004-provide-yydra-build.md): accepted; complete generation library, dedicated product api-build crate, and public build entrypoint; default backend/H5 with explicit Android; implemented in Distribution 0.4.0 and locally verified
- [Select local development validation by change](./docs/adr/0005-select-local-validation-by-change.md): accepted; active local agent workflow with Android triggered by native or build-chain changes; complete validation before release; CI subsequently amended by ADR 0006
- [Limit repository CI to CLI build and tests](./docs/adr/0006-limit-repository-ci-to-cli-build-and-tests.md): accepted; CLI build and command-behavior tests with independent DCO; implemented and locally verified; remote required-check migration pending
- [Store research and validation in the Wiki](./docs/adr/0007-store-research-and-validation-in-wiki.md): accepted documentation-storage decision; current rules remain in this repository; Wiki published and independently verified

- [Provide server containers with Compose](./docs/adr/0008-provide-server-containers-with-compose.md): accepted; Product-owned server image and persistent single-host Compose deployment, separate from the temporary development database
- [Provide reusable authentication with GitHub sign-in](./docs/adr/0009-provide-github-authentication-capability.md): accepted; independent product accounts/sessions, GitHub auto-provisioning, H5/Android template integration, and per-account Reading Queue data; implemented with published dependencies and SQLx 0.8.6; selected PostgreSQL, H5, Android build and emulator acceptance passed; live GitHub and full release conformance not claimed

## Historical research and validation

- [Wiki home](https://github.com/yydcnjjw/yydra/wiki/Home): research and dated validation records; current context and ADR authority remains above
- [Research index](https://github.com/yydcnjjw/yydra/wiki/Research-Index): dated decision inputs and pointers to subsequent decisions and implementation
