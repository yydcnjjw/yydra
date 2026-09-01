use std::env;
use std::fs;
use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let output = env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("contracts/openapi.json"));
    let openapi = product_transport_http::openapi();
    let mut document = serde_json::to_string_pretty(&openapi)?;
    document.push('\n');
    fs::write(output, document)?;
    Ok(())
}
