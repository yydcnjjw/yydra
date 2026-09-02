// SPDX-License-Identifier: MIT OR Apache-2.0

use std::env;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let database_url = env::var("DATABASE_URL")?;
    product_persistence_postgres::apply_migrations(&database_url).await
}
