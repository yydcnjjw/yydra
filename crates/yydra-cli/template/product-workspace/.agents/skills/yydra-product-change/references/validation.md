<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Project validation

Use the cheapest discriminating test first and expand coverage for affected behavior.

1. Run the changed unit test at its owning Rust or frontend surface.
2. Run affected integration and Public API contract tests with Cargo/npm.
3. Run formatters, linters, TypeScript checks, and affected application builds.
4. For PostgreSQL tests, prepare a fresh disposable database, migrate it, and
   run `cargo test --locked --test reading_queue_postgres -- --ignored --test-threads=1`.
5. For H5 tests, start a migrated backend, install Playwright Chromium explicitly,
   set `EXPO_PUBLIC_API_URL`, and run `npm --prefix frontend run test:e2e` or
   `npm --prefix frontend run test:product-semantics`.
6. Changes affecting native inputs or Android build tooling need
   `yydra build . --target android`. Keep its actual APK and relevant build log.

Use `yydra doctor .` to diagnose environments; it does not replace any test or
build. The Workspace README provides commands and disposable-database cleanup.
Report the commands actually run, their outcomes, and missing coverage. A build
alone does not prove native runtime, device behavior, accessibility, or Agent
performance. Preserve test expectations and generated/source ownership.
