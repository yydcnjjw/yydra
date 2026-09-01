//! Reading Queue concepts and state-transition rules.

#![forbid(unsafe_code)]

use std::fmt;
use std::str::FromStr;

use thiserror::Error;
use url::Url;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ReadingEntryId(Uuid);

impl ReadingEntryId {
    #[must_use]
    pub fn new() -> Self {
        Self(Uuid::now_v7())
    }

    #[must_use]
    pub const fn from_uuid(value: Uuid) -> Self {
        Self(value)
    }

    #[must_use]
    pub fn into_uuid(self) -> Uuid {
        self.0
    }
}

impl Default for ReadingEntryId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for ReadingEntryId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

impl FromStr for ReadingEntryId {
    type Err = uuid::Error;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Uuid::parse_str(value).map(Self)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadingStatus {
    Queued,
    Completed,
}

impl ReadingStatus {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Completed => "completed",
        }
    }
}

impl fmt::Display for ReadingStatus {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for ReadingStatus {
    type Err = DomainError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "queued" => Ok(Self::Queued),
            "completed" => Ok(Self::Completed),
            _ => Err(DomainError::UnknownStatus(value.to_owned())),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadingTransition {
    Complete,
    Reopen,
}

impl fmt::Display for ReadingTransition {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Complete => "complete",
            Self::Reopen => "reopen",
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadingEntry {
    id: ReadingEntryId,
    title: String,
    source_url: String,
    status: ReadingStatus,
}

impl ReadingEntry {
    pub fn new(
        title: impl Into<String>,
        source_url: impl Into<String>,
    ) -> Result<Self, DomainError> {
        let title = title.into().trim().to_owned();
        if title.is_empty() || title.chars().count() > 240 {
            return Err(DomainError::InvalidTitle);
        }
        let source_url = source_url.into();
        let parsed = Url::parse(&source_url).map_err(|_| DomainError::InvalidSourceUrl)?;
        if !matches!(parsed.scheme(), "http" | "https") || parsed.host_str().is_none() {
            return Err(DomainError::InvalidSourceUrl);
        }

        Ok(Self {
            id: ReadingEntryId::new(),
            title,
            source_url,
            status: ReadingStatus::Queued,
        })
    }

    pub fn from_persisted(
        id: ReadingEntryId,
        title: String,
        source_url: String,
        status: ReadingStatus,
    ) -> Result<Self, DomainError> {
        let mut entry = Self::new(title, source_url)?;
        entry.id = id;
        entry.status = status;
        Ok(entry)
    }

    pub fn complete(&mut self) -> Result<(), TransitionError> {
        self.apply(ReadingTransition::Complete)
    }

    pub fn reopen(&mut self) -> Result<(), TransitionError> {
        self.apply(ReadingTransition::Reopen)
    }

    fn apply(&mut self, transition: ReadingTransition) -> Result<(), TransitionError> {
        let next = match (self.status, transition) {
            (ReadingStatus::Queued, ReadingTransition::Complete) => ReadingStatus::Completed,
            (ReadingStatus::Completed, ReadingTransition::Reopen) => ReadingStatus::Queued,
            (current, attempted) => {
                return Err(TransitionError { current, attempted });
            }
        };
        self.status = next;
        Ok(())
    }

    #[must_use]
    pub const fn id(&self) -> ReadingEntryId {
        self.id
    }

    #[must_use]
    pub fn title(&self) -> &str {
        &self.title
    }

    #[must_use]
    pub fn source_url(&self) -> &str {
        &self.source_url
    }

    #[must_use]
    pub const fn status(&self) -> ReadingStatus {
        self.status
    }
}

#[derive(Debug, Error)]
pub enum DomainError {
    #[error("title must contain 1 to 240 characters")]
    InvalidTitle,
    #[error("source URL must be an absolute HTTP or HTTPS URL")]
    InvalidSourceUrl,
    #[error("unknown reading status '{0}'")]
    UnknownStatus(String),
}

#[derive(Debug, Error)]
#[error("cannot {attempted} a {current} reading entry")]
pub struct TransitionError {
    pub current: ReadingStatus,
    pub attempted: ReadingTransition,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_legal_transitions_change_state() {
        let mut entry = ReadingEntry::new("Yydra", "https://example.com/yydra").unwrap();
        entry.complete().unwrap();
        assert_eq!(entry.status(), ReadingStatus::Completed);
        assert!(entry.complete().is_err());
        entry.reopen().unwrap();
        assert_eq!(entry.status(), ReadingStatus::Queued);
        assert!(entry.reopen().is_err());
    }
}
