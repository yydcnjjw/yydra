# Stable rule routing

- `origin.exact-distribution`: install and invoke the exact recorded CLI only
  with authority to install it; never rewrite the record.
- `ownership.baseline-skills`: restore the exact Distribution snapshot. Do not
  preserve local edits inside Baseline Skills.
- `ownership.generated-boundaries`: move authored code out of generated paths
  or restore façade boundaries, then regenerate if needed.
- `rust.format`, `rust.compile`, `rust.clippy`, `rust.test`: repair the owning
  Rust source or test. Keep `SQLX_OFFLINE=true` compatibility for ordinary
  compile/test nodes.
- `api.openapi-drift`, `api.generated-client-drift`: change Axum source if the
  contract is wrong, then run `yydra generate api .`; never edit either output.
- `frontend.expo-dependencies`: reconcile with the pinned Expo SDK matrix
  deliberately. Do not accept an automatic major/downgrade.
- `frontend.advisories`: trace the advisory to the direct/transitive owner and
  test a pinned compatible remediation. Do not use forced audit repair.
- `frontend.typecheck`, `frontend.test`: repair authored frontend code or tests,
  not generated client output.
- `h5.production-export`: reproduce the Metro/static-export failure from the
  locked frontend before changing dependencies.
- `h5.product-presentation-accessibility`: repair the Product-owned semantic
  assertion or the direct React Native role/name/state implementation. Reproduce
  with `npm run test:product-semantics`; do not replace semantic locators with
  visual text or test-id assertions.

The local check may report database, H5 E2E, Android release, or iOS build
nodes as `not-run`. Execute those only in their declared environment and retain
separate build/runtime evidence; never reinterpret CNG generation as a build or
a build as runtime validation.
