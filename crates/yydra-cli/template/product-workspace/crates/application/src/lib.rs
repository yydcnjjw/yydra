// SPDX-License-Identifier: MIT OR Apache-2.0

//! Strongly typed Product Workspace use cases belong here.

#![forbid(unsafe_code)]

pub mod post_commit;

use std::error::Error;
use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use hmac::{Hmac, KeyInit, Mac};
use product_domain::{
    DomainValidationError, ReadingEntry, ReadingEntryId, ReadingEntryOrder, ReadingEntryState,
    ReadingEntryStatusFilter, ReadingEntryTitle, SourceUrl,
};
use product_persistence_postgres::{
    Database, PersistenceError, ReadingEntryPagePosition, adjust_reading_progress,
    insert_reading_entry, list_reading_entries_page, load_reading_progress,
    lock_reading_entry_for_update, update_reading_entry_state,
};
use serde::{Deserialize, Serialize};
use sha2::Sha256;

const CURSOR_VERSION: u8 = 1;
const MAX_CURSOR_LENGTH: usize = 2_048;
const MIN_CURSOR_SIGNING_KEY_LENGTH: usize = 32;
const DEFAULT_READING_QUEUE_PAGE_SIZE: u16 = 20;
const MAX_READING_QUEUE_PAGE_SIZE: u16 = 50;

type HmacSha256 = Hmac<Sha256>;

#[derive(Clone)]
pub struct HealthService {
    database: Database,
}

impl HealthService {
    pub fn new(database: Database) -> Self {
        Self { database }
    }

    pub async fn check(&self) -> Result<HealthStatus, Box<dyn std::error::Error>> {
        Ok(HealthStatus {
            status: "ready",
            database: self.database.schema_name().await?,
        })
    }
}

pub struct HealthStatus {
    pub status: &'static str,
    pub database: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CreateReadingEntryCommand {
    pub title: String,
    pub source_url: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReadingQueueEntryState {
    Queued,
    Completed,
}

impl From<ReadingEntryState> for ReadingQueueEntryState {
    fn from(state: ReadingEntryState) -> Self {
        match state {
            ReadingEntryState::Queued => Self::Queued,
            ReadingEntryState::Completed => Self::Completed,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReadingQueueEntry {
    pub id: String,
    pub title: String,
    pub source_url: String,
    pub state: ReadingQueueEntryState,
}

impl From<ReadingEntry> for ReadingQueueEntry {
    fn from(entry: ReadingEntry) -> Self {
        Self {
            id: entry.id().as_str().to_owned(),
            title: entry.title().as_str().to_owned(),
            source_url: entry.source_url().as_str().to_owned(),
            state: entry.state().into(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ReadingQueueCursorContext {
    status: ReadingEntryStatusFilter,
    order: ReadingEntryOrder,
    limit: u16,
    authorization_scope: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
struct ReadingQueueCursorPosition {
    created_at: String,
    id: String,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ReadingQueueCursorPayload {
    version: u8,
    status: String,
    sort: String,
    limit: u16,
    authorization_scope: String,
    position: ReadingQueueCursorPosition,
}

#[derive(Clone)]
struct CursorCodec {
    signing_key: Arc<[u8]>,
}

impl CursorCodec {
    fn new(signing_key: impl AsRef<[u8]>) -> Result<Self, CursorConfigurationError> {
        let signing_key = signing_key.as_ref();
        if signing_key.len() < MIN_CURSOR_SIGNING_KEY_LENGTH {
            return Err(CursorConfigurationError);
        }
        Ok(Self {
            signing_key: Arc::from(signing_key),
        })
    }

    fn encode(
        &self,
        context: &ReadingQueueCursorContext,
        position: &ReadingQueueCursorPosition,
    ) -> Result<String, CursorCodecError> {
        let payload = serde_json::to_vec(&ReadingQueueCursorPayload {
            version: CURSOR_VERSION,
            status: context.status.as_str().to_owned(),
            sort: context.order.as_str().to_owned(),
            limit: context.limit,
            authorization_scope: context.authorization_scope.clone(),
            position: position.clone(),
        })
        .map_err(|_| CursorCodecError)?;
        let mut mac =
            HmacSha256::new_from_slice(&self.signing_key).map_err(|_| CursorCodecError)?;
        mac.update(&payload);
        let signature = mac.finalize().into_bytes();
        Ok(format!(
            "v{CURSOR_VERSION}.{}.{}",
            URL_SAFE_NO_PAD.encode(payload),
            URL_SAFE_NO_PAD.encode(signature)
        ))
    }

    fn decode(
        &self,
        cursor: &str,
        context: &ReadingQueueCursorContext,
    ) -> Result<ReadingQueueCursorPosition, CursorCodecError> {
        if cursor.is_empty() || cursor.len() > MAX_CURSOR_LENGTH {
            return Err(CursorCodecError);
        }
        let mut parts = cursor.split('.');
        let version = parts.next().ok_or(CursorCodecError)?;
        let payload = parts.next().ok_or(CursorCodecError)?;
        let signature = parts.next().ok_or(CursorCodecError)?;
        if parts.next().is_some() || version != format!("v{CURSOR_VERSION}") {
            return Err(CursorCodecError);
        }
        let payload = URL_SAFE_NO_PAD
            .decode(payload)
            .map_err(|_| CursorCodecError)?;
        let signature = URL_SAFE_NO_PAD
            .decode(signature)
            .map_err(|_| CursorCodecError)?;
        let mut mac =
            HmacSha256::new_from_slice(&self.signing_key).map_err(|_| CursorCodecError)?;
        mac.update(&payload);
        mac.verify_slice(&signature).map_err(|_| CursorCodecError)?;
        let payload: ReadingQueueCursorPayload =
            serde_json::from_slice(&payload).map_err(|_| CursorCodecError)?;
        if payload.version != CURSOR_VERSION
            || payload.status != context.status.as_str()
            || payload.sort != context.order.as_str()
            || payload.limit != context.limit
            || payload.authorization_scope != context.authorization_scope
            || payload.position.created_at.is_empty()
            || payload.position.created_at.len() > 64
            || payload.position.created_at.chars().any(char::is_control)
            || ReadingEntryId::parse(payload.position.id.clone()).is_err()
        {
            return Err(CursorCodecError);
        }
        Ok(payload.position)
    }
}

#[derive(Debug)]
pub struct CursorConfigurationError;

impl fmt::Display for CursorConfigurationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "cursor signing key must contain at least {MIN_CURSOR_SIGNING_KEY_LENGTH} bytes"
        )
    }
}

impl Error for CursorConfigurationError {}

#[derive(Debug)]
struct CursorCodecError;

impl fmt::Display for CursorCodecError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("reading queue cursor is invalid")
    }
}

impl Error for CursorCodecError {}

#[derive(Clone)]
pub struct CreateReadingEntry {
    database: Database,
}

impl CreateReadingEntry {
    pub fn new(database: Database) -> Self {
        Self { database }
    }

    pub async fn execute(
        &self,
        command: CreateReadingEntryCommand,
    ) -> Result<ReadingQueueEntry, CreateReadingEntryError> {
        let title = ReadingEntryTitle::parse(command.title)?;
        let source_url = SourceUrl::parse(command.source_url)?;
        let mut transaction = self
            .database
            .begin()
            .await
            .map_err(CreateReadingEntryError::storage)?;
        let entry = match insert_reading_entry(&mut transaction, &title, &source_url).await {
            Ok(entry) => entry,
            Err(error) => {
                transaction
                    .rollback()
                    .await
                    .map_err(CreateReadingEntryError::storage)?;
                return Err(CreateReadingEntryError::storage(error));
            }
        };
        transaction
            .commit()
            .await
            .map_err(CreateReadingEntryError::storage)?;
        Ok(entry.into())
    }
}

#[derive(Clone)]
pub struct ListReadingEntries {
    database: Database,
    cursor_codec: CursorCodec,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ListReadingEntriesQuery {
    pub status: Option<String>,
    pub sort: Option<String>,
    pub limit: Option<u16>,
    pub cursor: Option<String>,
    pub authorization_scope: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReadingQueuePage {
    pub entries: Vec<ReadingQueueEntry>,
    pub next_cursor: Option<String>,
}

impl ListReadingEntries {
    pub fn new(
        database: Database,
        cursor_signing_key: impl AsRef<[u8]>,
    ) -> Result<Self, CursorConfigurationError> {
        Ok(Self {
            database,
            cursor_codec: CursorCodec::new(cursor_signing_key)?,
        })
    }

    pub async fn execute(
        &self,
        query: ListReadingEntriesQuery,
    ) -> Result<ReadingQueuePage, ListReadingEntriesError> {
        let status = ReadingEntryStatusFilter::parse(query.status.as_deref())
            .map_err(ListReadingEntriesError::invalid_input)?;
        let order = ReadingEntryOrder::parse(query.sort.as_deref())
            .map_err(ListReadingEntriesError::invalid_input)?;
        let limit = query.limit.unwrap_or(DEFAULT_READING_QUEUE_PAGE_SIZE);
        if !(1..=MAX_READING_QUEUE_PAGE_SIZE).contains(&limit) {
            return Err(ListReadingEntriesError::InvalidInput {
                field: "limit",
                message: "must be between 1 and 50",
            });
        }
        if query.authorization_scope.is_empty()
            || query.authorization_scope.len() > 256
            || query.authorization_scope.chars().any(char::is_control)
        {
            return Err(ListReadingEntriesError::InvalidInput {
                field: "authorization",
                message: "contains an invalid authorization scope",
            });
        }
        let context = ReadingQueueCursorContext {
            status,
            order,
            limit,
            authorization_scope: query.authorization_scope,
        };
        let after = query
            .cursor
            .as_deref()
            .map(|cursor| self.cursor_codec.decode(cursor, &context))
            .transpose()
            .map_err(|_| ListReadingEntriesError::InvalidCursor)?;
        let after = after.map(|position| ReadingEntryPagePosition {
            created_at: position.created_at,
            id: position.id,
        });
        let mut transaction = self
            .database
            .begin()
            .await
            .map_err(ListReadingEntriesError::storage)?;
        if let Err(error) = sqlx::query("SET TRANSACTION READ ONLY")
            .execute(&mut *transaction)
            .await
        {
            transaction
                .rollback()
                .await
                .map_err(ListReadingEntriesError::storage)?;
            return Err(ListReadingEntriesError::storage(error));
        }
        let mut items = match list_reading_entries_page(
            &mut transaction,
            status,
            order,
            after.as_ref(),
            limit + 1,
        )
        .await
        {
            Ok(items) => items,
            Err(error) => {
                transaction
                    .rollback()
                    .await
                    .map_err(ListReadingEntriesError::storage)?;
                return Err(ListReadingEntriesError::storage(error));
            }
        };
        let has_next_page = items.len() > usize::from(limit);
        if has_next_page {
            items.truncate(usize::from(limit));
        }
        transaction
            .commit()
            .await
            .map_err(ListReadingEntriesError::storage)?;
        let next_cursor = if has_next_page {
            let position = items
                .last()
                .map(|item| ReadingQueueCursorPosition {
                    created_at: item.position.created_at.clone(),
                    id: item.position.id.clone(),
                })
                .ok_or_else(|| ListReadingEntriesError::storage(CursorCodecError))?;
            Some(
                self.cursor_codec
                    .encode(&context, &position)
                    .map_err(ListReadingEntriesError::storage)?,
            )
        } else {
            None
        };
        Ok(ReadingQueuePage {
            entries: items.into_iter().map(|item| item.entry.into()).collect(),
            next_cursor,
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ChangeReadingEntryStateCommand {
    pub id: String,
    pub target: ReadingQueueEntryState,
}

#[derive(Clone)]
pub struct ChangeReadingEntryStateAndRecordProgress {
    database: Database,
}

impl ChangeReadingEntryStateAndRecordProgress {
    pub fn new(database: Database) -> Self {
        Self { database }
    }

    pub async fn execute(
        &self,
        command: ChangeReadingEntryStateCommand,
    ) -> Result<ReadingQueueEntry, ChangeReadingEntryStateError> {
        let id = ReadingEntryId::parse(command.id)?;
        let mut transaction = self
            .database
            .begin()
            .await
            .map_err(ChangeReadingEntryStateError::storage)?;
        let mut entry = match lock_reading_entry_for_update(&mut transaction, &id).await {
            Ok(Some(entry)) => entry,
            Ok(None) => {
                transaction
                    .rollback()
                    .await
                    .map_err(ChangeReadingEntryStateError::storage)?;
                return Err(ChangeReadingEntryStateError::NotFound {
                    id: id.as_str().to_owned(),
                });
            }
            Err(error) => {
                transaction
                    .rollback()
                    .await
                    .map_err(ChangeReadingEntryStateError::storage)?;
                return Err(ChangeReadingEntryStateError::storage(error));
            }
        };
        let transition = match command.target {
            ReadingQueueEntryState::Completed => entry.complete(),
            ReadingQueueEntryState::Queued => entry.reopen(),
        };
        if let Err(error) = transition {
            let conflict = ChangeReadingEntryStateError::Conflict {
                current: error.current().into(),
                requested: error.requested().into(),
            };
            transaction
                .rollback()
                .await
                .map_err(ChangeReadingEntryStateError::storage)?;
            return Err(conflict);
        }
        if let Err(error) = update_reading_entry_state(&mut transaction, &entry).await {
            transaction
                .rollback()
                .await
                .map_err(ChangeReadingEntryStateError::storage)?;
            return Err(ChangeReadingEntryStateError::storage(error));
        }
        let completed_delta = match command.target {
            ReadingQueueEntryState::Completed => 1,
            ReadingQueueEntryState::Queued => -1,
        };
        if let Err(error) = adjust_reading_progress(&mut transaction, completed_delta).await {
            transaction
                .rollback()
                .await
                .map_err(ChangeReadingEntryStateError::storage)?;
            return Err(ChangeReadingEntryStateError::storage(error));
        }
        transaction
            .commit()
            .await
            .map_err(ChangeReadingEntryStateError::storage)?;
        Ok(entry.into())
    }
}

pub type ChangeReadingEntryState = ChangeReadingEntryStateAndRecordProgress;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReadingProgressView {
    pub completed_entries: u64,
}

#[derive(Clone)]
pub struct GetReadingProgress {
    database: Database,
}

impl GetReadingProgress {
    pub fn new(database: Database) -> Self {
        Self { database }
    }

    pub async fn execute(&self) -> Result<ReadingProgressView, GetReadingProgressError> {
        let mut transaction = self
            .database
            .begin()
            .await
            .map_err(GetReadingProgressError::storage)?;
        if let Err(error) = sqlx::query("SET TRANSACTION READ ONLY")
            .execute(&mut *transaction)
            .await
        {
            transaction
                .rollback()
                .await
                .map_err(GetReadingProgressError::storage)?;
            return Err(GetReadingProgressError::storage(error));
        }
        let progress = match load_reading_progress(&mut transaction).await {
            Ok(progress) => progress,
            Err(error) => {
                transaction
                    .rollback()
                    .await
                    .map_err(GetReadingProgressError::storage)?;
                return Err(GetReadingProgressError::storage(error));
            }
        };
        transaction
            .commit()
            .await
            .map_err(GetReadingProgressError::storage)?;
        Ok(ReadingProgressView {
            completed_entries: progress.completed_entries(),
        })
    }
}

#[derive(Debug)]
pub enum CreateReadingEntryError {
    InvalidInput {
        field: &'static str,
        message: &'static str,
    },
    Storage(Box<dyn Error + Send + Sync>),
}

impl CreateReadingEntryError {
    fn storage(error: impl Error + Send + Sync + 'static) -> Self {
        Self::Storage(Box::new(error))
    }
}

impl From<DomainValidationError> for CreateReadingEntryError {
    fn from(error: DomainValidationError) -> Self {
        Self::InvalidInput {
            field: error.field(),
            message: error.message(),
        }
    }
}

impl fmt::Display for CreateReadingEntryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidInput { field, message } => write!(formatter, "{field} {message}"),
            Self::Storage(_) => formatter.write_str("reading entry storage failed"),
        }
    }
}

impl Error for CreateReadingEntryError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::InvalidInput { .. } => None,
            Self::Storage(error) => Some(error.as_ref()),
        }
    }
}

#[derive(Debug)]
pub enum ListReadingEntriesError {
    InvalidInput {
        field: &'static str,
        message: &'static str,
    },
    InvalidCursor,
    Storage(Box<dyn Error + Send + Sync>),
}

impl ListReadingEntriesError {
    fn invalid_input(error: DomainValidationError) -> Self {
        Self::InvalidInput {
            field: error.field(),
            message: error.message(),
        }
    }

    fn storage(error: impl Error + Send + Sync + 'static) -> Self {
        Self::Storage(Box::new(error))
    }
}

impl fmt::Display for ListReadingEntriesError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidInput { field, message } => write!(formatter, "{field} {message}"),
            Self::InvalidCursor => formatter.write_str("reading queue cursor is invalid"),
            Self::Storage(_) => formatter.write_str("reading queue storage failed"),
        }
    }
}

impl Error for ListReadingEntriesError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Storage(error) => Some(error.as_ref()),
            _ => None,
        }
    }
}

#[derive(Debug)]
pub enum GetReadingProgressError {
    Storage(Box<dyn Error + Send + Sync>),
}

impl GetReadingProgressError {
    fn storage(error: impl Error + Send + Sync + 'static) -> Self {
        Self::Storage(Box::new(error))
    }
}

impl fmt::Display for GetReadingProgressError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("reading progress storage failed")
    }
}

impl Error for GetReadingProgressError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Storage(error) => Some(error.as_ref()),
        }
    }
}

#[derive(Debug)]
pub enum ChangeReadingEntryStateError {
    InvalidInput {
        field: &'static str,
        message: &'static str,
    },
    NotFound {
        id: String,
    },
    Conflict {
        current: ReadingQueueEntryState,
        requested: ReadingQueueEntryState,
    },
    Storage(Box<dyn Error + Send + Sync>),
}

impl ChangeReadingEntryStateError {
    fn storage(error: impl Error + Send + Sync + 'static) -> Self {
        Self::Storage(Box::new(error))
    }
}

impl From<DomainValidationError> for ChangeReadingEntryStateError {
    fn from(error: DomainValidationError) -> Self {
        Self::InvalidInput {
            field: error.field(),
            message: error.message(),
        }
    }
}

impl fmt::Display for ChangeReadingEntryStateError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidInput { field, message } => write!(formatter, "{field} {message}"),
            Self::NotFound { id } => write!(formatter, "reading entry {id} was not found"),
            Self::Conflict { current, requested } => write!(
                formatter,
                "cannot transition reading entry from {current:?} to {requested:?}"
            ),
            Self::Storage(_) => formatter.write_str("reading entry transition storage failed"),
        }
    }
}

impl Error for ChangeReadingEntryStateError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Storage(error) => Some(error.as_ref()),
            _ => None,
        }
    }
}

impl From<PersistenceError> for ListReadingEntriesError {
    fn from(error: PersistenceError) -> Self {
        Self::storage(error)
    }
}

pub type ApplicationFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

pub trait ReadingQueueApplication: Send + Sync {
    fn create<'a>(
        &'a self,
        command: CreateReadingEntryCommand,
    ) -> ApplicationFuture<'a, Result<ReadingQueueEntry, CreateReadingEntryError>>;

    fn list<'a>(
        &'a self,
        query: ListReadingEntriesQuery,
    ) -> ApplicationFuture<'a, Result<ReadingQueuePage, ListReadingEntriesError>>;

    fn change<'a>(
        &'a self,
        command: ChangeReadingEntryStateCommand,
    ) -> ApplicationFuture<'a, Result<ReadingQueueEntry, ChangeReadingEntryStateError>>;
}

#[derive(Clone)]
pub struct ReadingQueueService {
    create: CreateReadingEntry,
    list: ListReadingEntries,
    change: ChangeReadingEntryState,
}

impl ReadingQueueService {
    pub fn new(
        database: Database,
        cursor_signing_key: impl AsRef<[u8]>,
    ) -> Result<Self, CursorConfigurationError> {
        Ok(Self {
            create: CreateReadingEntry::new(database.clone()),
            list: ListReadingEntries::new(database.clone(), cursor_signing_key)?,
            change: ChangeReadingEntryState::new(database),
        })
    }
}

impl ReadingQueueApplication for ReadingQueueService {
    fn create<'a>(
        &'a self,
        command: CreateReadingEntryCommand,
    ) -> ApplicationFuture<'a, Result<ReadingQueueEntry, CreateReadingEntryError>> {
        Box::pin(self.create.execute(command))
    }

    fn list<'a>(
        &'a self,
        query: ListReadingEntriesQuery,
    ) -> ApplicationFuture<'a, Result<ReadingQueuePage, ListReadingEntriesError>> {
        Box::pin(self.list.execute(query))
    }

    fn change<'a>(
        &'a self,
        command: ChangeReadingEntryStateCommand,
    ) -> ApplicationFuture<'a, Result<ReadingQueueEntry, ChangeReadingEntryStateError>> {
        Box::pin(self.change.execute(command))
    }
}

#[cfg(test)]
mod tests {
    use product_domain::{ReadingEntryOrder, ReadingEntryStatusFilter};

    use super::{CursorCodec, ReadingQueueCursorContext, ReadingQueueCursorPosition};

    #[test]
    fn cursor_is_url_safe_and_bound_to_query_and_authorization_context() {
        let codec =
            CursorCodec::new(b"0123456789abcdef0123456789abcdef").expect("valid signing key");
        let context = ReadingQueueCursorContext {
            status: ReadingEntryStatusFilter::Queued,
            order: ReadingEntryOrder::OldestFirst,
            limit: 2,
            authorization_scope: "anonymous".to_owned(),
        };
        let position = ReadingQueueCursorPosition {
            created_at: "2026-09-03 01:02:03+00".to_owned(),
            id: "00000000-0000-0000-0000-000000000002".to_owned(),
        };

        let cursor = codec.encode(&context, &position).expect("encode cursor");
        assert!(
            cursor
                .chars()
                .all(|character| character.is_ascii_alphanumeric() || "-_.".contains(character))
        );
        assert_eq!(
            codec.decode(&cursor, &context).expect("matching context"),
            position
        );

        let changed_filter = ReadingQueueCursorContext {
            status: ReadingEntryStatusFilter::Completed,
            ..context.clone()
        };
        assert!(codec.decode(&cursor, &changed_filter).is_err());
        let changed_order = ReadingQueueCursorContext {
            order: ReadingEntryOrder::NewestFirst,
            ..context.clone()
        };
        assert!(codec.decode(&cursor, &changed_order).is_err());
        let changed_authorization = ReadingQueueCursorContext {
            authorization_scope: "protected".to_owned(),
            ..context.clone()
        };
        assert!(codec.decode(&cursor, &changed_authorization).is_err());

        let mut tampered = cursor.into_bytes();
        let payload_byte = tampered.get_mut(4).expect("cursor payload");
        *payload_byte = if *payload_byte == b'A' { b'B' } else { b'A' };
        assert!(
            codec
                .decode(
                    &String::from_utf8(tampered).expect("ASCII cursor"),
                    &context
                )
                .is_err()
        );
    }
}
