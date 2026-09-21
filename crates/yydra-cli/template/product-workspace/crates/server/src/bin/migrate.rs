// SPDX-License-Identifier: MIT OR Apache-2.0

use snafu::Snafu;
use std::env;

#[tokio::main]
async fn main() -> std::process::ExitCode {
    match run().await {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("MIGRATION_FAILED: {error}");
            std::process::ExitCode::FAILURE
        }
    }
}

async fn run() -> Result<(), MigrationCommandError> {
    let database_url = env::var("DATABASE_URL")?;
    product_persistence_postgres::apply_migrations(&database_url).await?;
    Ok(())
}

#[derive(Debug, Snafu)]
enum MigrationCommandError {
    #[snafu(context(false), display("DATABASE_URL is required"))]
    Environment { source: std::env::VarError },
    #[snafu(context(false), display("applying product migrations failed"))]
    Migration {
        source: product_persistence_postgres::MigrationError,
    },
}
