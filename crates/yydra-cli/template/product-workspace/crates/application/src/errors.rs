// SPDX-License-Identifier: MIT OR Apache-2.0
use super::CursorCodecError;
use product_domain::{DomainTransitionError, DomainValidationError};
use product_persistence_postgres::PersistenceError;
use snafu::{Backtrace, GenerateImplicitData, Snafu};

#[derive(Debug, Snafu)]
#[snafu(display("database health check failed"))]
pub struct HealthError {
    pub(crate) source: sqlx::Error,
}

#[derive(Debug, Snafu)]
#[snafu(context(suffix(Cause)))]
pub enum StorageCause {
    #[snafu(context(false), display("database operation failed"))]
    Database { source: sqlx::Error },
    #[snafu(context(false), display("persistence operation failed"))]
    Persistence { source: PersistenceError },
    #[snafu(context(false), display("cursor encoding failed"))]
    Cursor { source: CursorCodecError },
}
impl StorageCause {
    pub fn code(&self) -> &'static str {
        match self {
            Self::Database { source } => database_code(source),
            Self::Persistence { source } => source.code(),
            Self::Cursor { .. } => "CURSOR_ENCODING_FAILED",
        }
    }
}
fn database_code(error: &sqlx::Error) -> &'static str {
    match error {
        sqlx::Error::PoolTimedOut => "DATABASE_POOL_TIMEOUT",
        sqlx::Error::PoolClosed => "DATABASE_POOL_CLOSED",
        sqlx::Error::Io(_) => "DATABASE_IO",
        sqlx::Error::Database(_) => "DATABASE_REJECTED",
        _ => "DATABASE_FAILED",
    }
}

#[derive(Debug, Snafu)]
#[snafu(context(suffix(CreateReadingEntryErrorContext)))]
pub enum CreateReadingEntryError {
    #[snafu(context(false), display("input violates a product rule"))]
    InvalidInput { source: DomainValidationError },
    #[snafu(display("{operation}: storage operation failed"))]
    Storage {
        operation: &'static str,
        source: StorageCause,
        backtrace: Option<Backtrace>,
    },
    #[snafu(display("transaction commit failed; outcome is unknown"))]
    CommitOutcomeUnknown {
        source: sqlx::Error,
        backtrace: Option<Backtrace>,
    },
    #[snafu(display("transaction rollback also failed"))]
    RollbackFailed {
        source: Box<Self>,
        rollback: sqlx::Error,
        backtrace: Option<Backtrace>,
    },
}
impl CreateReadingEntryError {
    pub(super) fn storage(operation: &'static str, source: impl Into<StorageCause>) -> Self {
        Self::Storage {
            operation,
            source: source.into(),
            backtrace: Option::<Backtrace>::generate(),
        }
    }
    pub(super) fn commit(source: sqlx::Error) -> Self {
        Self::CommitOutcomeUnknown {
            source,
            backtrace: Option::<Backtrace>::generate(),
        }
    }
    pub fn with_rollback(self, outcome: Result<(), sqlx::Error>) -> Self {
        match outcome {
            Ok(()) => self,
            Err(rollback) => Self::RollbackFailed {
                source: Box::new(self),
                rollback,
                backtrace: Option::<Backtrace>::generate(),
            },
        }
    }
    /// Contains only classified error metadata, never raw SQL values or input.
    pub fn diagnostic_context(&self) -> String {
        match self {
            Self::Storage {
                operation, source, ..
            } => format!("{operation}: {}", source.code()),
            Self::CommitOutcomeUnknown { source, .. } => {
                format!("commit outcome unknown: {}", database_code(source))
            }
            Self::RollbackFailed {
                source, rollback, ..
            } => format!(
                "original: {}; rollback: {}",
                source.diagnostic_context(),
                database_code(rollback)
            ),
            Self::InvalidInput { source } => format!("{}: {}", source.field(), source.code()),
        }
    }
}

#[derive(Debug, Snafu)]
#[snafu(context(suffix(ListReadingEntriesErrorContext)))]
pub enum ListReadingEntriesError {
    #[snafu(context(false), display("input violates a product rule"))]
    InvalidInput { source: DomainValidationError },
    #[snafu(display("page limit must be between 1 and 50"))]
    InvalidLimit,
    #[snafu(display("authorization scope is invalid"))]
    InvalidAuthorizationScope,
    #[snafu(display("cursor is invalid"))]
    InvalidCursor { source: CursorCodecError },
    #[snafu(display("{operation}: storage operation failed"))]
    Storage {
        operation: &'static str,
        source: StorageCause,
        backtrace: Option<Backtrace>,
    },
    #[snafu(display("transaction commit failed; outcome is unknown"))]
    CommitOutcomeUnknown {
        source: sqlx::Error,
        backtrace: Option<Backtrace>,
    },
    #[snafu(display("transaction rollback also failed"))]
    RollbackFailed {
        source: Box<Self>,
        rollback: sqlx::Error,
        backtrace: Option<Backtrace>,
    },
}
impl ListReadingEntriesError {
    pub(super) fn storage(operation: &'static str, source: impl Into<StorageCause>) -> Self {
        Self::Storage {
            operation,
            source: source.into(),
            backtrace: Option::<Backtrace>::generate(),
        }
    }
    pub(super) fn commit(source: sqlx::Error) -> Self {
        Self::CommitOutcomeUnknown {
            source,
            backtrace: Option::<Backtrace>::generate(),
        }
    }
    pub fn with_rollback(self, outcome: Result<(), sqlx::Error>) -> Self {
        match outcome {
            Ok(()) => self,
            Err(rollback) => Self::RollbackFailed {
                source: Box::new(self),
                rollback,
                backtrace: Option::<Backtrace>::generate(),
            },
        }
    }
    /// Contains only classified error metadata, never raw SQL values or input.
    pub fn diagnostic_context(&self) -> String {
        match self {
            Self::Storage {
                operation, source, ..
            } => format!("{operation}: {}", source.code()),
            Self::CommitOutcomeUnknown { source, .. } => {
                format!("commit outcome unknown: {}", database_code(source))
            }
            Self::RollbackFailed {
                source, rollback, ..
            } => format!(
                "original: {}; rollback: {}",
                source.diagnostic_context(),
                database_code(rollback)
            ),
            Self::InvalidInput { source } => format!("{}: {}", source.field(), source.code()),
            Self::InvalidLimit => "invalid page limit".into(),
            Self::InvalidAuthorizationScope => "invalid authorization scope".into(),
            Self::InvalidCursor { .. } => "invalid cursor".into(),
        }
    }
}

#[derive(Debug, Snafu)]
#[snafu(context(suffix(GetReadingProgressErrorContext)))]
pub enum GetReadingProgressError {
    #[snafu(display("{operation}: storage operation failed"))]
    Storage {
        operation: &'static str,
        source: StorageCause,
        backtrace: Option<Backtrace>,
    },
    #[snafu(display("transaction commit failed; outcome is unknown"))]
    CommitOutcomeUnknown {
        source: sqlx::Error,
        backtrace: Option<Backtrace>,
    },
    #[snafu(display("transaction rollback also failed"))]
    RollbackFailed {
        source: Box<Self>,
        rollback: sqlx::Error,
        backtrace: Option<Backtrace>,
    },
}
impl GetReadingProgressError {
    pub(super) fn storage(operation: &'static str, source: impl Into<StorageCause>) -> Self {
        Self::Storage {
            operation,
            source: source.into(),
            backtrace: Option::<Backtrace>::generate(),
        }
    }
    pub(super) fn commit(source: sqlx::Error) -> Self {
        Self::CommitOutcomeUnknown {
            source,
            backtrace: Option::<Backtrace>::generate(),
        }
    }
    pub fn with_rollback(self, outcome: Result<(), sqlx::Error>) -> Self {
        match outcome {
            Ok(()) => self,
            Err(rollback) => Self::RollbackFailed {
                source: Box::new(self),
                rollback,
                backtrace: Option::<Backtrace>::generate(),
            },
        }
    }
    /// Contains only classified error metadata, never raw SQL values or input.
    pub fn diagnostic_context(&self) -> String {
        match self {
            Self::Storage {
                operation, source, ..
            } => format!("{operation}: {}", source.code()),
            Self::CommitOutcomeUnknown { source, .. } => {
                format!("commit outcome unknown: {}", database_code(source))
            }
            Self::RollbackFailed {
                source, rollback, ..
            } => format!(
                "original: {}; rollback: {}",
                source.diagnostic_context(),
                database_code(rollback)
            ),
        }
    }
}

#[derive(Debug, Snafu)]
#[snafu(context(suffix(ChangeReadingEntryStateErrorContext)))]
pub enum ChangeReadingEntryStateError {
    #[snafu(context(false), display("input violates a product rule"))]
    InvalidInput { source: DomainValidationError },
    #[snafu(display("reading entry was not found"))]
    NotFound { id: String },
    #[snafu(context(false), display("requested state transition is not allowed"))]
    Conflict { source: DomainTransitionError },
    #[snafu(display("{operation}: storage operation failed"))]
    Storage {
        operation: &'static str,
        source: StorageCause,
        backtrace: Option<Backtrace>,
    },
    #[snafu(display("transaction commit failed; outcome is unknown"))]
    CommitOutcomeUnknown {
        source: sqlx::Error,
        backtrace: Option<Backtrace>,
    },
    #[snafu(display("transaction rollback also failed"))]
    RollbackFailed {
        source: Box<Self>,
        rollback: sqlx::Error,
        backtrace: Option<Backtrace>,
    },
}
impl ChangeReadingEntryStateError {
    pub(super) fn storage(operation: &'static str, source: impl Into<StorageCause>) -> Self {
        Self::Storage {
            operation,
            source: source.into(),
            backtrace: Option::<Backtrace>::generate(),
        }
    }
    pub(super) fn commit(source: sqlx::Error) -> Self {
        Self::CommitOutcomeUnknown {
            source,
            backtrace: Option::<Backtrace>::generate(),
        }
    }
    pub fn with_rollback(self, outcome: Result<(), sqlx::Error>) -> Self {
        match outcome {
            Ok(()) => self,
            Err(rollback) => Self::RollbackFailed {
                source: Box::new(self),
                rollback,
                backtrace: Option::<Backtrace>::generate(),
            },
        }
    }
    /// Contains only classified error metadata, never raw SQL values or input.
    pub fn diagnostic_context(&self) -> String {
        match self {
            Self::Storage {
                operation, source, ..
            } => format!("{operation}: {}", source.code()),
            Self::CommitOutcomeUnknown { source, .. } => {
                format!("commit outcome unknown: {}", database_code(source))
            }
            Self::RollbackFailed {
                source, rollback, ..
            } => format!(
                "original: {}; rollback: {}",
                source.diagnostic_context(),
                database_code(rollback)
            ),
            Self::InvalidInput { source } => format!("{}: {}", source.field(), source.code()),
            Self::NotFound { .. } => "entry not found".into(),
            Self::Conflict { .. } => "transition conflict".into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::error::Error;
    #[test]
    fn rollback_failure_retains_original_rule_and_secondary_database_error() {
        let error = CreateReadingEntryError::from(DomainValidationError::EmptyTitle)
            .with_rollback(Err(sqlx::Error::PoolClosed));
        let CreateReadingEntryError::RollbackFailed {
            source, rollback, ..
        } = &error
        else {
            panic!("technical failure required")
        };
        assert!(matches!(
            **source,
            CreateReadingEntryError::InvalidInput {
                source: DomainValidationError::EmptyTitle
            }
        ));
        assert!(matches!(rollback, sqlx::Error::PoolClosed));
        assert!(error.source().unwrap().source().is_some());
        assert!(error.diagnostic_context().contains("empty"));
        assert!(error.diagnostic_context().contains("DATABASE_POOL_CLOSED"));
    }
}
