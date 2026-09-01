//! Reading Queue PostgreSQL persistence adapter.

#![forbid(unsafe_code)]

use product_domain::{DomainError, ReadingEntry, ReadingEntryId, ReadingStatus};
use sqlx::{PgConnection, PgPool};
use thiserror::Error;
use uuid::Uuid;

pub static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("../../migrations");

#[derive(Debug, sqlx::FromRow)]
pub struct PersistedEntry {
    pub id: Uuid,
    pub title: String,
    pub source_url: String,
    pub status: String,
    pub created_at_micros: i64,
}

impl PersistedEntry {
    pub fn into_domain(self) -> Result<ReadingEntry, PersistenceError> {
        let status = self
            .status
            .parse()
            .map_err(PersistenceError::CorruptDomainData)?;
        ReadingEntry::from_persisted(
            ReadingEntryId::from_uuid(self.id),
            self.title,
            self.source_url,
            status,
        )
        .map_err(PersistenceError::CorruptDomainData)
    }
}

pub async fn insert(
    connection: &mut PgConnection,
    entry: &ReadingEntry,
) -> Result<PersistedEntry, PersistenceError> {
    sqlx::query_as!(
        PersistedEntry,
        r#"
        INSERT INTO reading_entries (id, title, source_url, status)
        VALUES ($1, $2, $3, $4)
        RETURNING id, title, source_url, status,
          floor(extract(epoch FROM created_at) * 1000000)::bigint AS "created_at_micros!"
        "#,
        entry.id().into_uuid(),
        entry.title(),
        entry.source_url(),
        entry.status().as_str(),
    )
    .fetch_one(connection)
    .await
    .map_err(PersistenceError::Sqlx)
}

pub async fn find_for_update(
    connection: &mut PgConnection,
    id: ReadingEntryId,
) -> Result<Option<PersistedEntry>, PersistenceError> {
    sqlx::query_as!(
        PersistedEntry,
        r#"
        SELECT id, title, source_url, status,
          floor(extract(epoch FROM created_at) * 1000000)::bigint AS "created_at_micros!"
        FROM reading_entries
        WHERE id = $1
        FOR UPDATE
        "#,
        id.into_uuid(),
    )
    .fetch_optional(connection)
    .await
    .map_err(PersistenceError::Sqlx)
}

pub async fn update_status(
    connection: &mut PgConnection,
    id: ReadingEntryId,
    status: ReadingStatus,
) -> Result<PersistedEntry, PersistenceError> {
    sqlx::query_as!(
        PersistedEntry,
        r#"
        UPDATE reading_entries
        SET status = $2, updated_at = CURRENT_TIMESTAMP
        WHERE id = $1
        RETURNING id, title, source_url, status,
          floor(extract(epoch FROM created_at) * 1000000)::bigint AS "created_at_micros!"
        "#,
        id.into_uuid(),
        status.as_str(),
    )
    .fetch_one(connection)
    .await
    .map_err(PersistenceError::Sqlx)
}

pub async fn list(
    pool: &PgPool,
    status: Option<ReadingStatus>,
    after: Option<(i64, Uuid)>,
    limit: i64,
) -> Result<Vec<PersistedEntry>, PersistenceError> {
    let (after_micros, after_id) = after.unzip();
    sqlx::query_as!(
        PersistedEntry,
        r#"
        SELECT id, title, source_url, status,
          floor(extract(epoch FROM created_at) * 1000000)::bigint AS "created_at_micros!"
        FROM reading_entries
        WHERE ($1::text IS NULL OR status = $1)
          AND (
            $2::bigint IS NULL
            OR floor(extract(epoch FROM created_at) * 1000000)::bigint < $2
            OR (
              floor(extract(epoch FROM created_at) * 1000000)::bigint = $2
              AND id < $3::uuid
            )
          )
        ORDER BY created_at DESC, id DESC
        LIMIT $4
        "#,
        status.map(ReadingStatus::as_str),
        after_micros,
        after_id,
        limit,
    )
    .fetch_all(pool)
    .await
    .map_err(PersistenceError::Sqlx)
}

pub async fn check_migration_state(pool: &PgPool) -> Result<(), PersistenceError> {
    let applied = sqlx::query_as!(
        AppliedMigration,
        "SELECT version, checksum FROM _sqlx_migrations ORDER BY version"
    )
    .fetch_all(pool)
    .await
    .map_err(PersistenceError::Sqlx)?;

    let expected = MIGRATOR.iter().collect::<Vec<_>>();
    if applied.len() != expected.len() {
        return Err(PersistenceError::MigrationState(format!(
            "expected {} applied migrations, found {}",
            expected.len(),
            applied.len()
        )));
    }
    for (applied, expected) in applied.iter().zip(expected) {
        if applied.version != expected.version || applied.checksum != expected.checksum.as_ref() {
            return Err(PersistenceError::MigrationState(format!(
                "migration {} does not match the compiled source",
                expected.version
            )));
        }
    }
    Ok(())
}

#[derive(Debug, sqlx::FromRow)]
struct AppliedMigration {
    version: i64,
    checksum: Vec<u8>,
}

#[derive(Debug, Error)]
pub enum PersistenceError {
    #[error("database operation failed")]
    Sqlx(#[source] sqlx::Error),
    #[error("database contains invalid Product Domain data: {0}")]
    CorruptDomainData(#[source] DomainError),
    #[error("database migration state is incompatible: {0}")]
    MigrationState(String),
}
