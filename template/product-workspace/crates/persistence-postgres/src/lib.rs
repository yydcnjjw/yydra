//! Product-owned PostgreSQL persistence adapters.

#![forbid(unsafe_code)]

use std::io;

use sqlx::PgPool;

pub static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("../../migrations");

pub async fn check_migration_state(pool: &PgPool) -> Result<(), Box<dyn std::error::Error>> {
    let applied = sqlx::query_as::<_, AppliedMigration>(
        "SELECT version, checksum FROM _sqlx_migrations ORDER BY version",
    )
    .fetch_all(pool)
    .await?;
    let expected = MIGRATOR.iter().collect::<Vec<_>>();
    if applied.len() != expected.len() {
        return Err(io::Error::other(format!(
            "expected {} applied migrations, found {}",
            expected.len(),
            applied.len()
        ))
        .into());
    }
    for (applied, expected) in applied.iter().zip(expected) {
        if applied.version != expected.version || applied.checksum != expected.checksum.as_ref() {
            return Err(io::Error::other(format!(
                "migration {} does not match the compiled source",
                expected.version
            ))
            .into());
        }
    }
    Ok(())
}

#[derive(Debug, sqlx::FromRow)]
struct AppliedMigration {
    version: i64,
    checksum: Vec<u8>,
}
