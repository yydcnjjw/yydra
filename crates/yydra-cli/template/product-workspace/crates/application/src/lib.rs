// SPDX-License-Identifier: MIT OR Apache-2.0

//! Strongly typed Product Workspace use cases belong here.

#![forbid(unsafe_code)]

use std::error::Error;
use std::fmt;
use std::future::Future;
use std::pin::Pin;

use product_domain::{
    DomainValidationError, ReadingEntry, ReadingEntryId, ReadingEntryState, ReadingEntryTitle,
    SourceUrl,
};
use product_persistence_postgres::{
    Database, PersistenceError, insert_reading_entry, list_reading_entries,
    lock_reading_entry_for_update, update_reading_entry_state,
};

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
}

impl ListReadingEntries {
    pub fn new(database: Database) -> Self {
        Self { database }
    }

    pub async fn execute(&self) -> Result<Vec<ReadingQueueEntry>, ListReadingEntriesError> {
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
        let entries = match list_reading_entries(&mut transaction).await {
            Ok(entries) => entries,
            Err(error) => {
                transaction
                    .rollback()
                    .await
                    .map_err(ListReadingEntriesError::storage)?;
                return Err(ListReadingEntriesError::storage(error));
            }
        };
        transaction
            .commit()
            .await
            .map_err(ListReadingEntriesError::storage)?;
        Ok(entries.into_iter().map(Into::into).collect())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ChangeReadingEntryStateCommand {
    pub id: String,
    pub target: ReadingQueueEntryState,
}

#[derive(Clone)]
pub struct ChangeReadingEntryState {
    database: Database,
}

impl ChangeReadingEntryState {
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
        transaction
            .commit()
            .await
            .map_err(ChangeReadingEntryStateError::storage)?;
        Ok(entry.into())
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
pub struct ListReadingEntriesError(Box<dyn Error + Send + Sync>);

impl ListReadingEntriesError {
    fn storage(error: impl Error + Send + Sync + 'static) -> Self {
        Self(Box::new(error))
    }
}

impl fmt::Display for ListReadingEntriesError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("reading queue storage failed")
    }
}

impl Error for ListReadingEntriesError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(self.0.as_ref())
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

    fn list(
        &self,
    ) -> ApplicationFuture<'_, Result<Vec<ReadingQueueEntry>, ListReadingEntriesError>>;

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
    pub fn new(database: Database) -> Self {
        Self {
            create: CreateReadingEntry::new(database.clone()),
            list: ListReadingEntries::new(database.clone()),
            change: ChangeReadingEntryState::new(database),
        }
    }
}

impl ReadingQueueApplication for ReadingQueueService {
    fn create<'a>(
        &'a self,
        command: CreateReadingEntryCommand,
    ) -> ApplicationFuture<'a, Result<ReadingQueueEntry, CreateReadingEntryError>> {
        Box::pin(self.create.execute(command))
    }

    fn list(
        &self,
    ) -> ApplicationFuture<'_, Result<Vec<ReadingQueueEntry>, ListReadingEntriesError>> {
        Box::pin(self.list.execute())
    }

    fn change<'a>(
        &'a self,
        command: ChangeReadingEntryStateCommand,
    ) -> ApplicationFuture<'a, Result<ReadingQueueEntry, ChangeReadingEntryStateError>> {
        Box::pin(self.change.execute(command))
    }
}
