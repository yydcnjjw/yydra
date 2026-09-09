<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# yydra-build

Public build support for Yydra Product Workspaces. A product's dedicated
`api-build` package supplies its current Rust-derived OpenAPI to `generate_api`.
The library validates it, runs project-local Orval and TypeScript, and writes
the Generated Client beneath the build script's Cargo `OUT_DIR`.

```rust,ignore
yydra_build::generate_api(&openapi, &yydra_build::ApiBuild {
    frontend: &frontend,
    out_dir: &out_dir,
})?;
```

The library never invokes Cargo or links into frontend dependencies. The product
preparation script performs those steps, including restoring missing outputs
and frontend links. Errors stop consumers; outputs remain disposable.

Yydra's CLI package includes the same library sources as an exact Distribution
snapshot at `.yydra/build-support`. New Product Workspaces use that pinned path
dependency, so creating and building a candidate does not require publishing
the library first. `doctor` and `check` verify the snapshot. The library is also
independently packageable as `yydra-build`; its canonical source is maintained
in `crates/yydra-build` in the Yydra repository.
