// SPDX-License-Identifier: MIT OR Apache-2.0

//! Product-owned PostgreSQL persistence adapters.

#![forbid(unsafe_code)]

use std::io;

use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;

static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("../../migrations");

#[derive(Clone)]
pub struct Database {
    pool: PgPool,
}

impl Database {
    pub async fn connect(database_url: &str, max_connections: u32) -> Result<Self, sqlx::Error> {
        let pool = PgPoolOptions::new()
            .max_connections(max_connections)
            .connect(database_url)
            .await?;
        Ok(Self { pool })
    }

    pub async fn verify_compiled_migrations(&self) -> Result<(), Box<dyn std::error::Error>> {
        let applied = sqlx::query_as::<_, AppliedMigration>(
            "SELECT version, checksum, success FROM _sqlx_migrations ORDER BY version",
        )
        .fetch_all(&self.pool)
        .await?;
        let expected = MIGRATOR
            .iter()
            .map(|migration| ExpectedMigration {
                version: migration.version,
                checksum: migration.checksum.as_ref().to_vec(),
            })
            .collect::<Vec<_>>();
        verify_history(&applied, &expected)
    }

    pub async fn schema_name(&self) -> Result<String, sqlx::Error> {
        sqlx::query_scalar(
            "SELECT schema_name FROM yydra_workspace_metadata WHERE singleton = TRUE",
        )
        .fetch_one(&self.pool)
        .await
    }
}

pub async fn apply_migrations(database_url: &str) -> Result<(), Box<dyn std::error::Error>> {
    let database = Database::connect(database_url, 1).await?;
    MIGRATOR.run(&database.pool).await?;
    Ok(())
}

fn verify_history(
    applied: &[AppliedMigration],
    expected: &[ExpectedMigration],
) -> Result<(), Box<dyn std::error::Error>> {
    if applied.len() < expected.len() {
        return Err(io::Error::other(format!(
            "database migration history is missing {} compiled migration(s)",
            expected.len() - applied.len()
        ))
        .into());
    }
    if applied.len() > expected.len() {
        return Err(io::Error::other(format!(
            "database migration history contains {} unknown migration(s)",
            applied.len() - expected.len()
        ))
        .into());
    }
    for (applied, expected) in applied.iter().zip(expected) {
        if !applied.success {
            return Err(io::Error::other(format!(
                "database migration {} is not successful",
                applied.version
            ))
            .into());
        }
        if applied.version != expected.version {
            return Err(io::Error::other(format!(
                "database migration version {} is incompatible with compiled version {}",
                applied.version, expected.version
            ))
            .into());
        }
        if applied.checksum != expected.checksum {
            return Err(io::Error::other(format!(
                "database migration {} checksum differs from compiled source",
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
    success: bool,
}

struct ExpectedMigration {
    version: i64,
    checksum: Vec<u8>,
}

#[cfg(test)]
mod tests {
    use super::{AppliedMigration, ExpectedMigration, verify_history};

    fn expected() -> Vec<ExpectedMigration> {
        vec![ExpectedMigration {
            version: 1,
            checksum: vec![1, 2, 3],
        }]
    }

    fn applied() -> Vec<AppliedMigration> {
        vec![AppliedMigration {
            version: 1,
            checksum: vec![1, 2, 3],
            success: true,
        }]
    }

    #[test]
    fn accepts_the_exact_compiled_history() {
        verify_history(&applied(), &expected()).expect("exact history");
    }

    #[test]
    fn rejects_missing_unknown_mutated_and_unsuccessful_history() {
        assert!(verify_history(&[], &expected()).is_err());

        let mut unknown = applied();
        unknown.push(AppliedMigration {
            version: 2,
            checksum: vec![4],
            success: true,
        });
        assert!(verify_history(&unknown, &expected()).is_err());

        let mut mutated = applied();
        mutated[0].checksum = vec![9];
        assert!(verify_history(&mutated, &expected()).is_err());

        let mut unsuccessful = applied();
        unsuccessful[0].success = false;
        assert!(verify_history(&unsuccessful, &expected()).is_err());
    }
}
