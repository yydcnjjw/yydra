// SPDX-License-Identifier: MIT OR Apache-2.0

use snafu::Snafu;
use std::path::PathBuf;

fn main() -> std::process::ExitCode {
    match run() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("API_BUILD_FAILED: {error}");
            std::process::ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), ApiBuildError> {
    let frontend = PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").ok_or(
        ApiBuildError::MissingVariable {
            name: "CARGO_MANIFEST_DIR",
        },
    )?)
    .join("../../frontend");
    let out_dir = PathBuf::from(
        std::env::var_os("OUT_DIR").ok_or(ApiBuildError::MissingVariable { name: "OUT_DIR" })?,
    );
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

#[derive(Debug, Snafu)]
enum ApiBuildError {
    #[snafu(display("Cargo must provide {name}"))]
    MissingVariable { name: &'static str },
    #[snafu(context(false), display("exporting the public API failed"))]
    Export {
        source: product_transport_http::OpenApiExportError,
    },
    #[snafu(context(false), display("generating the API client failed: {source}"))]
    Generate { source: yydra_build::Error },
}
