// SPDX-License-Identifier: MIT OR Apache-2.0

//! Product-owned PostgreSQL persistence adapters.

#![forbid(unsafe_code)]

use std::error::Error;
use std::fmt;
use std::io;

use product_domain::{
    DomainValidationError, ReadingEntry, ReadingEntryId, ReadingEntryOrder,
    ReadingEntryStatusFilter, ReadingEntryTitle, ReadingProgress, SourceUrl,
};
use sqlx::postgres::PgPoolOptions;
use sqlx::{PgConnection, PgPool, Postgres, Transaction};

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

    pub fn pool(&self) -> PgPool {
        self.pool.clone()
    }

    pub async fn begin(&self) -> Result<Transaction<'_, Postgres>, sqlx::Error> {
        self.pool.begin().await
    }
}

pub async fn insert_reading_entry(
    connection: &mut PgConnection,
    account_id: &str,
    title: &ReadingEntryTitle,
    source_url: &SourceUrl,
) -> Result<ReadingEntry, PersistenceError> {
    sqlx::query("INSERT INTO reading_progress (account_id, completed_entries) VALUES ($1::uuid, 0) ON CONFLICT (account_id) DO NOTHING")
        .bind(account_id).execute(&mut *connection).await?;
    let row = sqlx::query_as::<_, ReadingEntryRow>(
        r#"
        INSERT INTO reading_queue_entries (title, source_url, account_id)
        VALUES ($1, $2, $3::uuid)
        RETURNING id::text AS id, title, source_url, state
        "#,
    )
    .bind(title.as_str())
    .bind(source_url.as_str())
    .bind(account_id)
    .fetch_one(connection)
    .await?;
    row.try_into()
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReadingEntryPagePosition {
    pub created_at: String,
    pub id: String,
}

pub struct ReadingEntryPageItem {
    pub entry: ReadingEntry,
    pub position: ReadingEntryPagePosition,
}

pub async fn list_reading_entries_page(
    connection: &mut PgConnection,
    account_id: &str,
    status: ReadingEntryStatusFilter,
    order: ReadingEntryOrder,
    after: Option<&ReadingEntryPagePosition>,
    fetch_limit: u16,
) -> Result<Vec<ReadingEntryPageItem>, PersistenceError> {
    let status = status.persisted_state();
    let after_created_at = after.map(|position| position.created_at.as_str());
    let after_id = after.map(|position| position.id.as_str());
    let rows = match order {
        ReadingEntryOrder::OldestFirst => {
            sqlx::query_as::<_, ReadingEntryPageRow>(
                r#"
                SELECT
                    id::text AS id,
                    title,
                    source_url,
                    state,
                    to_char(
                        created_at AT TIME ZONE 'UTC',
                        'YYYY-MM-DD"T"HH24:MI:SS.US"Z"'
                    ) AS created_at_cursor
                FROM reading_queue_entries
                WHERE account_id = $5::uuid AND ($1::text IS NULL OR state = $1)
                  AND (
                    $2::timestamptz IS NULL
                    OR (created_at, id) > ($2::timestamptz, $3::uuid)
                  )
                ORDER BY created_at ASC, id ASC
                LIMIT $4
                "#,
            )
            .bind(status)
            .bind(after_created_at)
            .bind(after_id)
            .bind(i64::from(fetch_limit))
            .bind(account_id)
            .fetch_all(connection)
            .await?
        }
        ReadingEntryOrder::NewestFirst => {
            sqlx::query_as::<_, ReadingEntryPageRow>(
                r#"
                SELECT
                    id::text AS id,
                    title,
                    source_url,
                    state,
                    to_char(
                        created_at AT TIME ZONE 'UTC',
                        'YYYY-MM-DD"T"HH24:MI:SS.US"Z"'
                    ) AS created_at_cursor
                FROM reading_queue_entries
                WHERE account_id = $5::uuid AND ($1::text IS NULL OR state = $1)
                  AND (
                    $2::timestamptz IS NULL
                    OR (created_at, id) < ($2::timestamptz, $3::uuid)
                  )
                ORDER BY created_at DESC, id DESC
                LIMIT $4
                "#,
            )
            .bind(status)
            .bind(after_created_at)
            .bind(after_id)
            .bind(i64::from(fetch_limit))
            .bind(account_id)
            .fetch_all(connection)
            .await?
        }
    };
    rows.into_iter().map(TryInto::try_into).collect()
}

pub async fn lock_reading_entry_for_update(
    connection: &mut PgConnection,
    account_id: &str,
    id: &ReadingEntryId,
) -> Result<Option<ReadingEntry>, PersistenceError> {
    sqlx::query_as::<_, ReadingEntryRow>(
        r#"
        SELECT id::text AS id, title, source_url, state
        FROM reading_queue_entries
        WHERE id::text = $1 AND account_id = $2::uuid
        FOR UPDATE
        "#,
    )
    .bind(id.as_str())
    .bind(account_id)
    .fetch_optional(connection)
    .await?
    .map(TryInto::try_into)
    .transpose()
}

pub async fn update_reading_entry_state(
    connection: &mut PgConnection,
    account_id: &str,
    entry: &ReadingEntry,
) -> Result<(), PersistenceError> {
    let updated = sqlx::query(
        r#"
        UPDATE reading_queue_entries
        SET state = $2
        WHERE id::text = $1 AND account_id = $3::uuid
        "#,
    )
    .bind(entry.id().as_str())
    .bind(entry.state().as_str())
    .bind(account_id)
    .execute(connection)
    .await?;
    if updated.rows_affected() != 1 {
        return Err(PersistenceError::ConcurrentChange);
    }
    Ok(())
}

pub async fn load_reading_progress(
    connection: &mut PgConnection,
    account_id: &str,
) -> Result<ReadingProgress, PersistenceError> {
    let (completed_entries, has_entries) = sqlx::query_as::<_, (Option<i64>, bool)>(
        "SELECT (SELECT completed_entries FROM reading_progress WHERE account_id = $1::uuid),
                EXISTS (SELECT 1 FROM reading_queue_entries WHERE account_id = $1::uuid)",
    )
    .bind(account_id)
    .fetch_one(connection)
    .await?;
    let completed_entries = match (completed_entries, has_entries) {
        (Some(value), _) => value,
        (None, false) => 0,
        (None, true) => {
            return Err(PersistenceError::InvariantUnavailable(
                "reading progress for the account is missing",
            ));
        }
    };
    ReadingProgress::restore(completed_entries).map_err(Into::into)
}

pub async fn adjust_reading_progress(
    connection: &mut PgConnection,
    account_id: &str,
    completed_delta: i64,
) -> Result<ReadingProgress, PersistenceError> {
    let completed_entries = sqlx::query_scalar::<_, i64>(
        r#"
        UPDATE reading_progress
        SET completed_entries = completed_entries + $1
        WHERE account_id = $2::uuid AND completed_entries + $1 >= 0
        RETURNING completed_entries
        "#,
    )
    .bind(completed_delta)
    .bind(account_id)
    .fetch_optional(connection)
    .await?
    .ok_or(PersistenceError::InvariantUnavailable(
        "reading progress could not apply the transition",
    ))?;
    ReadingProgress::restore(completed_entries).map_err(Into::into)
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

#[derive(Debug, sqlx::FromRow)]
struct ReadingEntryRow {
    id: String,
    title: String,
    source_url: String,
    state: String,
}

#[derive(Debug, sqlx::FromRow)]
struct ReadingEntryPageRow {
    id: String,
    title: String,
    source_url: String,
    state: String,
    created_at_cursor: String,
}

impl TryFrom<ReadingEntryPageRow> for ReadingEntryPageItem {
    type Error = PersistenceError;

    fn try_from(row: ReadingEntryPageRow) -> Result<Self, Self::Error> {
        let position = ReadingEntryPagePosition {
            created_at: row.created_at_cursor,
            id: row.id.clone(),
        };
        let entry = ReadingEntryRow {
            id: row.id,
            title: row.title,
            source_url: row.source_url,
            state: row.state,
        }
        .try_into()?;
        Ok(Self { entry, position })
    }
}

impl TryFrom<ReadingEntryRow> for ReadingEntry {
    type Error = PersistenceError;

    fn try_from(row: ReadingEntryRow) -> Result<Self, Self::Error> {
        Ok(ReadingEntry::restore(
            ReadingEntryId::parse(row.id)?,
            ReadingEntryTitle::parse(row.title)?,
            SourceUrl::parse(row.source_url)?,
            &row.state,
        )?)
    }
}

#[derive(Debug)]
pub enum PersistenceError {
    Database(sqlx::Error),
    CorruptDomain(DomainValidationError),
    ConcurrentChange,
    InvariantUnavailable(&'static str),
}

impl fmt::Display for PersistenceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Database(error) => write!(formatter, "PostgreSQL operation failed: {error}"),
            Self::CorruptDomain(error) => {
                write!(
                    formatter,
                    "persisted Product Domain state is invalid: {error}"
                )
            }
            Self::ConcurrentChange => {
                formatter.write_str("reading entry changed after it was locked")
            }
            Self::InvariantUnavailable(message) => formatter.write_str(message),
        }
    }
}

impl Error for PersistenceError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Database(error) => Some(error),
            Self::CorruptDomain(error) => Some(error),
            Self::ConcurrentChange | Self::InvariantUnavailable(_) => None,
        }
    }
}

impl From<sqlx::Error> for PersistenceError {
    fn from(error: sqlx::Error) -> Self {
        Self::Database(error)
    }
}

impl From<DomainValidationError> for PersistenceError {
    fn from(error: DomainValidationError) -> Self {
        Self::CorruptDomain(error)
    }
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
