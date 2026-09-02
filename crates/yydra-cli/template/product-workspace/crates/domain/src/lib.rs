// SPDX-License-Identifier: MIT OR Apache-2.0

//! Product-owned concepts and rules belong here.
//!
//! The bounded Reading Queue slice is ordinary product-owned source that teams may evolve.

#![forbid(unsafe_code)]

use std::error::Error;
use std::fmt;

const MAX_READING_ENTRY_ID_LENGTH: usize = 128;
const MAX_READING_ENTRY_TITLE_LENGTH: usize = 200;
const MAX_SOURCE_URL_LENGTH: usize = 2_048;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReadingEntryId(String);

impl ReadingEntryId {
    pub fn parse(value: impl Into<String>) -> Result<Self, DomainValidationError> {
        let value = value.into();
        if value.is_empty()
            || value.chars().count() > MAX_READING_ENTRY_ID_LENGTH
            || value.chars().any(char::is_whitespace)
            || value.chars().any(char::is_control)
        {
            return Err(DomainValidationError::new(
                "id",
                "must be a non-empty opaque identifier without whitespace",
            ));
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReadingEntryTitle(String);

impl ReadingEntryTitle {
    pub fn parse(value: impl Into<String>) -> Result<Self, DomainValidationError> {
        let value = value.into();
        let normalized = value.trim();
        if normalized.is_empty() {
            return Err(DomainValidationError::new("title", "must not be empty"));
        }
        if normalized.chars().count() > MAX_READING_ENTRY_TITLE_LENGTH {
            return Err(DomainValidationError::new(
                "title",
                "must contain at most 200 characters",
            ));
        }
        if normalized.chars().any(char::is_control) {
            return Err(DomainValidationError::new(
                "title",
                "must not contain control characters",
            ));
        }
        Ok(Self(normalized.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourceUrl(String);

impl SourceUrl {
    pub fn parse(value: impl Into<String>) -> Result<Self, DomainValidationError> {
        let value = value.into();
        let normalized = value.trim();
        if normalized.chars().count() > MAX_SOURCE_URL_LENGTH {
            return Err(DomainValidationError::new(
                "sourceUrl",
                "must contain at most 2048 characters",
            ));
        }
        if normalized.chars().any(char::is_whitespace) || normalized.chars().any(char::is_control) {
            return Err(DomainValidationError::new(
                "sourceUrl",
                "must not contain whitespace or control characters",
            ));
        }
        let remainder = normalized
            .strip_prefix("https://")
            .or_else(|| normalized.strip_prefix("http://"))
            .ok_or_else(|| {
                DomainValidationError::new("sourceUrl", "must use the http or https scheme")
            })?;
        let authority = remainder.split(['/', '?', '#']).next().unwrap_or_default();
        if authority.is_empty()
            || !authority
                .chars()
                .any(|character| character.is_ascii_alphanumeric())
        {
            return Err(DomainValidationError::new(
                "sourceUrl",
                "must contain a host",
            ));
        }
        Ok(Self(normalized.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReadingEntryState {
    Queued,
    Completed,
}

impl ReadingEntryState {
    pub fn parse_persisted(value: &str) -> Result<Self, DomainValidationError> {
        match value {
            "queued" => Ok(Self::Queued),
            "completed" => Ok(Self::Completed),
            _ => Err(DomainValidationError::new(
                "state",
                "contains an unknown persisted reading-entry state",
            )),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Completed => "completed",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReadingEntryStatusFilter {
    All,
    Queued,
    Completed,
}

impl ReadingEntryStatusFilter {
    pub fn parse(value: Option<&str>) -> Result<Self, DomainValidationError> {
        match value.unwrap_or("all") {
            "all" => Ok(Self::All),
            "queued" => Ok(Self::Queued),
            "completed" => Ok(Self::Completed),
            _ => Err(DomainValidationError::new(
                "status",
                "must be all, queued, or completed",
            )),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::Queued => "queued",
            Self::Completed => "completed",
        }
    }

    pub fn persisted_state(self) -> Option<&'static str> {
        match self {
            Self::All => None,
            Self::Queued => Some(ReadingEntryState::Queued.as_str()),
            Self::Completed => Some(ReadingEntryState::Completed.as_str()),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReadingEntryOrder {
    OldestFirst,
    NewestFirst,
}

impl ReadingEntryOrder {
    pub fn parse(value: Option<&str>) -> Result<Self, DomainValidationError> {
        match value.unwrap_or("oldest") {
            "oldest" => Ok(Self::OldestFirst),
            "newest" => Ok(Self::NewestFirst),
            _ => Err(DomainValidationError::new(
                "sort",
                "must be oldest or newest",
            )),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::OldestFirst => "oldest",
            Self::NewestFirst => "newest",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReadingEntry {
    id: ReadingEntryId,
    title: ReadingEntryTitle,
    source_url: SourceUrl,
    state: ReadingEntryState,
}

impl ReadingEntry {
    pub fn restore(
        id: ReadingEntryId,
        title: ReadingEntryTitle,
        source_url: SourceUrl,
        persisted_state: &str,
    ) -> Result<Self, DomainValidationError> {
        Ok(Self {
            id,
            title,
            source_url,
            state: ReadingEntryState::parse_persisted(persisted_state)?,
        })
    }

    pub fn id(&self) -> &ReadingEntryId {
        &self.id
    }

    pub fn title(&self) -> &ReadingEntryTitle {
        &self.title
    }

    pub fn source_url(&self) -> &SourceUrl {
        &self.source_url
    }

    pub fn state(&self) -> ReadingEntryState {
        self.state
    }

    pub fn complete(&mut self) -> Result<(), DomainTransitionError> {
        self.transition(ReadingEntryState::Queued, ReadingEntryState::Completed)
    }

    pub fn reopen(&mut self) -> Result<(), DomainTransitionError> {
        self.transition(ReadingEntryState::Completed, ReadingEntryState::Queued)
    }

    fn transition(
        &mut self,
        expected: ReadingEntryState,
        requested: ReadingEntryState,
    ) -> Result<(), DomainTransitionError> {
        if self.state != expected {
            return Err(DomainTransitionError {
                current: self.state,
                requested,
            });
        }
        self.state = requested;
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DomainTransitionError {
    current: ReadingEntryState,
    requested: ReadingEntryState,
}

impl DomainTransitionError {
    pub fn current(&self) -> ReadingEntryState {
        self.current
    }

    pub fn requested(&self) -> ReadingEntryState {
        self.requested
    }
}

impl fmt::Display for DomainTransitionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "cannot transition reading entry from {} to {}",
            self.current.as_str(),
            self.requested.as_str()
        )
    }
}

impl Error for DomainTransitionError {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DomainValidationError {
    field: &'static str,
    message: &'static str,
}

impl DomainValidationError {
    const fn new(field: &'static str, message: &'static str) -> Self {
        Self { field, message }
    }

    pub fn field(&self) -> &'static str {
        self.field
    }

    pub fn message(&self) -> &'static str {
        self.message
    }
}

impl fmt::Display for DomainValidationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{} {}", self.field, self.message)
    }
}

impl Error for DomainValidationError {}

#[cfg(test)]
mod tests {
    use super::{
        ReadingEntry, ReadingEntryId, ReadingEntryOrder, ReadingEntryState,
        ReadingEntryStatusFilter, ReadingEntryTitle, SourceUrl,
    };

    #[test]
    fn reading_queue_query_context_accepts_only_stable_status_and_order_values() {
        assert_eq!(
            ReadingEntryStatusFilter::parse(None).expect("default status filter"),
            ReadingEntryStatusFilter::All
        );
        assert_eq!(
            ReadingEntryStatusFilter::parse(Some("completed")).expect("completed filter"),
            ReadingEntryStatusFilter::Completed
        );
        assert!(ReadingEntryStatusFilter::parse(Some("done")).is_err());

        assert_eq!(
            ReadingEntryOrder::parse(None).expect("default order"),
            ReadingEntryOrder::OldestFirst
        );
        assert_eq!(
            ReadingEntryOrder::parse(Some("newest")).expect("newest order"),
            ReadingEntryOrder::NewestFirst
        );
        assert!(ReadingEntryOrder::parse(Some("title")).is_err());
    }

    #[test]
    fn reading_queue_values_normalize_valid_input_and_reject_invalid_input() {
        let title = ReadingEntryTitle::parse("  Rust for Rustaceans  ")
            .expect("a non-empty title within the limit");
        assert_eq!(title.as_str(), "Rust for Rustaceans");
        assert!(ReadingEntryTitle::parse("   ").is_err());
        assert!(ReadingEntryTitle::parse("x".repeat(201)).is_err());

        let source = SourceUrl::parse(" https://example.test/books/rust?edition=2 ")
            .expect("an HTTP source URL within the limit");
        assert_eq!(source.as_str(), "https://example.test/books/rust?edition=2");
        for invalid in [
            "ftp://example.test/book",
            "https:///missing-host",
            "https://example.test/has space",
        ] {
            assert!(SourceUrl::parse(invalid).is_err(), "accepted {invalid:?}");
        }
    }

    #[test]
    fn a_new_entry_is_queued_and_persisted_state_is_fail_closed() {
        let entry = ReadingEntry::restore(
            ReadingEntryId::parse("opaque-entry-1").expect("opaque identifier"),
            ReadingEntryTitle::parse("A useful article").expect("title"),
            SourceUrl::parse("https://example.test/article").expect("source URL"),
            "queued",
        )
        .expect("known persisted state");

        assert_eq!(entry.id().as_str(), "opaque-entry-1");
        assert_eq!(entry.state(), ReadingEntryState::Queued);
        assert!(
            ReadingEntry::restore(
                ReadingEntryId::parse("opaque-entry-2").expect("opaque identifier"),
                ReadingEntryTitle::parse("Another article").expect("title"),
                SourceUrl::parse("http://example.test/another").expect("source URL"),
                "invented-state",
            )
            .is_err()
        );
    }

    #[test]
    fn only_complete_and_reopen_transitions_are_allowed() {
        let mut entry = ReadingEntry::restore(
            ReadingEntryId::parse("opaque-entry-3").expect("opaque identifier"),
            ReadingEntryTitle::parse("State transitions").expect("title"),
            SourceUrl::parse("https://example.test/transitions").expect("source URL"),
            "queued",
        )
        .expect("queued entry");

        entry.complete().expect("queued entry can complete");
        assert_eq!(entry.state(), ReadingEntryState::Completed);
        assert!(entry.complete().is_err(), "completed cannot complete twice");
        entry.reopen().expect("completed entry can reopen");
        assert_eq!(entry.state(), ReadingEntryState::Queued);
        assert!(entry.reopen().is_err(), "queued cannot reopen twice");
    }
}
