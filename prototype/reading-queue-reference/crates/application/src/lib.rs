//! Strongly typed Reading Queue use cases and transaction boundaries.

#![forbid(unsafe_code)]

use std::str::FromStr;

use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use product_domain::{ReadingEntry, ReadingEntryId, ReadingStatus, ReadingTransition};
use product_persistence_postgres::{self as persistence, PersistedEntry, PersistenceError};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug)]
pub struct CreateReadingEntry {
    pub title: String,
    pub source_url: String,
}

#[derive(Debug)]
pub struct ReadingEntryView {
    pub id: String,
    pub title: String,
    pub source_url: String,
    pub status: ReadingStatus,
    cursor_position: (i64, Uuid),
}

impl ReadingEntryView {
    fn from_persisted(value: PersistedEntry) -> Result<Self, AppError> {
        let cursor_position = (value.created_at_micros, value.id);
        let entry = value.into_domain()?;
        Ok(Self {
            id: entry.id().to_string(),
            title: entry.title().to_owned(),
            source_url: entry.source_url().to_owned(),
            status: entry.status(),
            cursor_position,
        })
    }
}

#[derive(Debug)]
pub struct ReadingEntryPage {
    pub items: Vec<ReadingEntryView>,
    pub next_cursor: Option<String>,
}

pub async fn create_entry(
    pool: &PgPool,
    input: CreateReadingEntry,
) -> Result<ReadingEntryView, AppError> {
    let entry = ReadingEntry::new(input.title, input.source_url)
        .map_err(|error| AppError::InvalidInput(error.to_string()))?;
    let mut transaction = pool.begin().await.map_err(PersistenceError::Sqlx)?;
    let persisted = persistence::insert(&mut transaction, &entry).await?;
    transaction.commit().await.map_err(PersistenceError::Sqlx)?;
    ReadingEntryView::from_persisted(persisted)
}

pub async fn transition_entry(
    pool: &PgPool,
    id: &str,
    transition: ReadingTransition,
) -> Result<ReadingEntryView, AppError> {
    let id = ReadingEntryId::from_str(id).map_err(|_| AppError::NotFound)?;
    let mut transaction = pool.begin().await.map_err(PersistenceError::Sqlx)?;
    let persisted = persistence::find_for_update(&mut transaction, id)
        .await?
        .ok_or(AppError::NotFound)?;
    let mut entry = persisted.into_domain()?;
    let result = match transition {
        ReadingTransition::Complete => entry.complete(),
        ReadingTransition::Reopen => entry.reopen(),
    };
    if let Err(error) = result {
        return Err(AppError::InvalidTransition {
            current: error.current,
            attempted: error.attempted,
        });
    }
    let persisted = persistence::update_status(&mut transaction, id, entry.status()).await?;
    transaction.commit().await.map_err(PersistenceError::Sqlx)?;
    ReadingEntryView::from_persisted(persisted)
}

pub async fn list_entries(
    pool: &PgPool,
    status: Option<&str>,
    cursor: Option<&str>,
    limit: Option<u16>,
) -> Result<ReadingEntryPage, AppError> {
    let status = status
        .map(ReadingStatus::from_str)
        .transpose()
        .map_err(|error| AppError::InvalidInput(error.to_string()))?;
    let limit = limit.unwrap_or(20);
    if !(1..=50).contains(&limit) {
        return Err(AppError::InvalidInput(
            "limit must be between 1 and 50".to_owned(),
        ));
    }
    let after = cursor
        .map(|value| decode_cursor(value, status))
        .transpose()?;
    let mut rows = persistence::list(pool, status, after, i64::from(limit) + 1).await?;
    let has_more = rows.len() > usize::from(limit);
    rows.truncate(usize::from(limit));
    let items = rows
        .into_iter()
        .map(ReadingEntryView::from_persisted)
        .collect::<Result<Vec<_>, _>>()?;
    let next_cursor = if has_more {
        items
            .last()
            .map(|entry| encode_cursor(entry.cursor_position, status))
            .transpose()?
    } else {
        None
    };

    Ok(ReadingEntryPage { items, next_cursor })
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CursorV1 {
    version: u8,
    status: Option<String>,
    created_at_micros: i64,
    id: Uuid,
}

fn encode_cursor(position: (i64, Uuid), status: Option<ReadingStatus>) -> Result<String, AppError> {
    let payload = CursorV1 {
        version: 1,
        status: status.map(|value| value.as_str().to_owned()),
        created_at_micros: position.0,
        id: position.1,
    };
    let json = serde_json::to_vec(&payload).map_err(|_| AppError::InvalidCursor)?;
    Ok(URL_SAFE_NO_PAD.encode(json))
}

fn decode_cursor(value: &str, status: Option<ReadingStatus>) -> Result<(i64, Uuid), AppError> {
    let bytes = URL_SAFE_NO_PAD
        .decode(value)
        .map_err(|_| AppError::InvalidCursor)?;
    let payload: CursorV1 = serde_json::from_slice(&bytes).map_err(|_| AppError::InvalidCursor)?;
    if payload.version != 1 || payload.status.as_deref() != status.map(ReadingStatus::as_str) {
        return Err(AppError::InvalidCursor);
    }
    Ok((payload.created_at_micros, payload.id))
}

#[derive(Debug, Error)]
pub enum AppError {
    #[error("invalid input: {0}")]
    InvalidInput(String),
    #[error("reading entry not found")]
    NotFound,
    #[error("cannot {attempted} a {current} reading entry")]
    InvalidTransition {
        current: ReadingStatus,
        attempted: ReadingTransition,
    },
    #[error("invalid or context-mismatched cursor")]
    InvalidCursor,
    #[error(transparent)]
    Persistence(#[from] PersistenceError),
}
