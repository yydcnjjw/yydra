// SPDX-License-Identifier: MIT OR Apache-2.0

//! Strongly typed Product Workspace use cases belong here.

#![forbid(unsafe_code)]

pub mod post_commit;

use snafu::Snafu;
mod errors;
pub use errors::*;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use hmac::{Hmac, KeyInit, Mac};
use product_domain::{
    ReadingEntry, ReadingEntryId, ReadingEntryOrder, ReadingEntryState, ReadingEntryStatusFilter,
    ReadingEntryTitle, SourceUrl,
};
use product_persistence_postgres::{
    Database, ReadingEntryPagePosition, adjust_reading_progress, insert_reading_entry,
    list_reading_entries_page, load_reading_progress, lock_reading_entry_for_update,
    update_reading_entry_state,
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

    pub async fn check(&self) -> Result<HealthStatus, HealthError> {
        Ok(HealthStatus {
            status: "ready",
            database: self
                .database
                .schema_name()
                .await
                .map_err(|source| HealthError { source })?,
        })
    }
}

pub struct HealthStatus {
    pub status: &'static str,
    pub database: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CreateReadingEntryCommand {
    pub account_id: String,
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
        .map_err(|source| CursorCodecError::Json { source })?;
        let mut mac = HmacSha256::new_from_slice(&self.signing_key)
            .map_err(|source| CursorCodecError::SigningKey { source })?;
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
            return Err(CursorCodecError::InvalidEnvelope);
        }
        let mut parts = cursor.split('.');
        let version = parts.next().ok_or(CursorCodecError::InvalidEnvelope)?;
        let payload = parts.next().ok_or(CursorCodecError::InvalidEnvelope)?;
        let signature = parts.next().ok_or(CursorCodecError::InvalidEnvelope)?;
        if parts.next().is_some() || version != format!("v{CURSOR_VERSION}") {
            return Err(CursorCodecError::InvalidEnvelope);
        }
        let payload = URL_SAFE_NO_PAD
            .decode(payload)
            .map_err(|source| CursorCodecError::Base64 { source })?;
        let signature = URL_SAFE_NO_PAD
            .decode(signature)
            .map_err(|source| CursorCodecError::Base64 { source })?;
        let mut mac = HmacSha256::new_from_slice(&self.signing_key)
            .map_err(|source| CursorCodecError::SigningKey { source })?;
        mac.update(&payload);
        mac.verify_slice(&signature)
            .map_err(|source| CursorCodecError::Signature { source })?;
        let payload: ReadingQueueCursorPayload =
            serde_json::from_slice(&payload).map_err(|source| CursorCodecError::Json { source })?;
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
            return Err(CursorCodecError::InvalidEnvelope);
        }
        Ok(payload.position)
    }
}

#[derive(Debug, Snafu)]
#[snafu(display("cursor signing key must contain at least {MIN_CURSOR_SIGNING_KEY_LENGTH} bytes"))]
pub struct CursorConfigurationError;

#[derive(Debug, Snafu)]
#[snafu(context(suffix(CursorContext)))]
pub enum CursorCodecError {
    #[snafu(display("cursor envelope or query binding is invalid"))]
    InvalidEnvelope,
    #[snafu(display("cursor page position is missing"))]
    MissingPosition,
    #[snafu(display("cursor JSON encoding is invalid"))]
    Json { source: serde_json::Error },
    #[snafu(display("cursor base64 encoding is invalid"))]
    Base64 { source: base64::DecodeError },
    #[snafu(display("cursor signing key is invalid"))]
    SigningKey { source: hmac::digest::InvalidLength },
    #[snafu(display("cursor signature is invalid"))]
    Signature { source: hmac::digest::MacError },
}

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
            .map_err(|source| CreateReadingEntryError::storage("begin", source))?;
        let entry =
            match insert_reading_entry(&mut transaction, &command.account_id, &title, &source_url)
                .await
            {
                Ok(entry) => entry,
                Err(error) => {
                    return Err(CreateReadingEntryError::storage("execute", error)
                        .with_rollback(transaction.rollback().await));
                }
            };
        transaction
            .commit()
            .await
            .map_err(CreateReadingEntryError::commit)?;
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
            .map_err(ListReadingEntriesError::from)?;
        let order = ReadingEntryOrder::parse(query.sort.as_deref())
            .map_err(ListReadingEntriesError::from)?;
        let limit = query.limit.unwrap_or(DEFAULT_READING_QUEUE_PAGE_SIZE);
        if !(1..=MAX_READING_QUEUE_PAGE_SIZE).contains(&limit) {
            return Err(ListReadingEntriesError::InvalidLimit);
        }
        if query.authorization_scope.is_empty()
            || query.authorization_scope.len() > 256
            || query.authorization_scope.chars().any(char::is_control)
        {
            return Err(ListReadingEntriesError::InvalidAuthorizationScope);
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
            .map_err(|source| ListReadingEntriesError::InvalidCursor { source })?;
        let after = after.map(|position| ReadingEntryPagePosition {
            created_at: position.created_at,
            id: position.id,
        });
        let mut transaction = self
            .database
            .begin()
            .await
            .map_err(|source| ListReadingEntriesError::storage("begin", source))?;
        if let Err(error) = sqlx::query("SET TRANSACTION READ ONLY")
            .execute(&mut *transaction)
            .await
        {
            return Err(ListReadingEntriesError::storage("execute", error)
                .with_rollback(transaction.rollback().await));
        }
        let mut items = match list_reading_entries_page(
            &mut transaction,
            &context.authorization_scope,
            status,
            order,
            after.as_ref(),
            limit + 1,
        )
        .await
        {
            Ok(items) => items,
            Err(error) => {
                return Err(ListReadingEntriesError::storage("execute", error)
                    .with_rollback(transaction.rollback().await));
            }
        };
        let has_next_page = items.len() > usize::from(limit);
        if has_next_page {
            items.truncate(usize::from(limit));
        }
        transaction
            .commit()
            .await
            .map_err(ListReadingEntriesError::commit)?;
        let next_cursor = if has_next_page {
            let position = items
                .last()
                .map(|item| ReadingQueueCursorPosition {
                    created_at: item.position.created_at.clone(),
                    id: item.position.id.clone(),
                })
                .ok_or_else(|| {
                    ListReadingEntriesError::storage(
                        "encode_cursor",
                        CursorCodecError::MissingPosition,
                    )
                })?;
            Some(
                self.cursor_codec
                    .encode(&context, &position)
                    .map_err(|source| ListReadingEntriesError::storage("encode_cursor", source))?,
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
    pub account_id: String,
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
            .map_err(|source| ChangeReadingEntryStateError::storage("begin", source))?;
        let mut entry =
            match lock_reading_entry_for_update(&mut transaction, &command.account_id, &id).await {
                Ok(Some(entry)) => entry,
                Ok(None) => {
                    let error = ChangeReadingEntryStateError::NotFound {
                        id: id.as_str().to_owned(),
                    };
                    return Err(error.with_rollback(transaction.rollback().await));
                }
                Err(error) => {
                    return Err(ChangeReadingEntryStateError::storage("execute", error)
                        .with_rollback(transaction.rollback().await));
                }
            };
        let transition = match command.target {
            ReadingQueueEntryState::Completed => entry.complete(),
            ReadingQueueEntryState::Queued => entry.reopen(),
        };
        if let Err(error) = transition {
            let conflict = ChangeReadingEntryStateError::Conflict { source: error };
            return Err(conflict.with_rollback(transaction.rollback().await));
        }
        if let Err(error) =
            update_reading_entry_state(&mut transaction, &command.account_id, &entry).await
        {
            return Err(ChangeReadingEntryStateError::storage("execute", error)
                .with_rollback(transaction.rollback().await));
        }
        let completed_delta = match command.target {
            ReadingQueueEntryState::Completed => 1,
            ReadingQueueEntryState::Queued => -1,
        };
        if let Err(error) =
            adjust_reading_progress(&mut transaction, &command.account_id, completed_delta).await
        {
            return Err(ChangeReadingEntryStateError::storage("execute", error)
                .with_rollback(transaction.rollback().await));
        }
        transaction
            .commit()
            .await
            .map_err(ChangeReadingEntryStateError::commit)?;
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

    pub async fn execute(
        &self,
        account_id: &str,
    ) -> Result<ReadingProgressView, GetReadingProgressError> {
        let mut transaction = self
            .database
            .begin()
            .await
            .map_err(|source| GetReadingProgressError::storage("begin", source))?;
        if let Err(error) = sqlx::query("SET TRANSACTION READ ONLY")
            .execute(&mut *transaction)
            .await
        {
            return Err(GetReadingProgressError::storage("execute", error)
                .with_rollback(transaction.rollback().await));
        }
        let progress = match load_reading_progress(&mut transaction, account_id).await {
            Ok(progress) => progress,
            Err(error) => {
                return Err(GetReadingProgressError::storage("execute", error)
                    .with_rollback(transaction.rollback().await));
            }
        };
        transaction
            .commit()
            .await
            .map_err(GetReadingProgressError::commit)?;
        Ok(ReadingProgressView {
            completed_entries: progress.completed_entries(),
        })
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
