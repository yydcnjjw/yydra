// SPDX-License-Identifier: MIT OR Apache-2.0

#![forbid(unsafe_code)]

use std::env;
use std::fs;
use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let output = env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .ok_or("usage: export-openapi <output-path>")?;
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(output, product_transport_http::normalized_openapi_json()?)?;
    Ok(())
}
