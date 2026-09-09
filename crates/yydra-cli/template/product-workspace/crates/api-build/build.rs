// SPDX-License-Identifier: MIT OR Apache-2.0

use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let frontend = PathBuf::from(
        std::env::var_os("CARGO_MANIFEST_DIR").ok_or("Cargo manifest directory missing")?,
    )
    .join("../../frontend");
    let out_dir = PathBuf::from(std::env::var_os("OUT_DIR").ok_or("Cargo OUT_DIR missing")?);
    let openapi = product_transport_http::normalized_openapi_json()?;
    yydra_build::generate_api(
        openapi.as_bytes(),
        &yydra_build::ApiBuild {
            frontend: &frontend,
            out_dir: &out_dir,
        },
    )?;
    Ok(())
}
