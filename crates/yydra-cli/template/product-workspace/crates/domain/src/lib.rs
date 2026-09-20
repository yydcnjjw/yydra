// SPDX-License-Identifier: MIT OR Apache-2.0

//! Product-owned concepts and rules belong here.
//!
//! The bounded Reading Queue slice is ordinary product-owned source that teams may evolve.

#![forbid(unsafe_code)]

use snafu::Snafu;

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
            return Err(DomainValidationError::InvalidId);
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
            return Err(DomainValidationError::EmptyTitle);
        }
        if normalized.chars().count() > MAX_READING_ENTRY_TITLE_LENGTH {
            return Err(DomainValidationError::TitleTooLong);
        }
        if normalized.chars().any(char::is_control) {
            return Err(DomainValidationError::TitleControlCharacter);
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
            return Err(DomainValidationError::SourceUrlTooLong);
        }
        if normalized.chars().any(char::is_whitespace) || normalized.chars().any(char::is_control) {
            return Err(DomainValidationError::SourceUrlCharacters);
        }
        let remainder = normalized
            .strip_prefix("https://")
            .or_else(|| normalized.strip_prefix("http://"))
            .ok_or(DomainValidationError::SourceUrlScheme)?;
        let authority = remainder.split(['/', '?', '#']).next().unwrap_or_default();
        if authority.is_empty()
            || !authority
                .chars()
                .any(|character| character.is_ascii_alphanumeric())
        {
            return Err(DomainValidationError::SourceUrlHost);
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
            _ => Err(DomainValidationError::UnknownState),
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
            _ => Err(DomainValidationError::InvalidStatus),
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
            _ => Err(DomainValidationError::InvalidOrder),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::OldestFirst => "oldest",
            Self::NewestFirst => "newest",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReadingProgress {
    completed_entries: u64,
}

impl ReadingProgress {
    pub fn restore(completed_entries: i64) -> Result<Self, DomainValidationError> {
        Ok(Self {
            completed_entries: u64::try_from(completed_entries)
                .map_err(|_| DomainValidationError::NegativeProgress)?,
        })
    }

    pub fn completed_entries(self) -> u64 {
        self.completed_entries
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

#[derive(Clone, Copy, Debug, Eq, PartialEq, Snafu)]
#[snafu(display("cannot transition reading entry from {} to {}", current.as_str(), requested.as_str()))]
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

/// A product rule, independent of how the candidate state reached the domain.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Snafu)]
pub enum DomainValidationError {
    #[snafu(display("id must be a non-empty opaque identifier without whitespace"))]
    InvalidId,
    #[snafu(display("title must not be empty"))]
    EmptyTitle,
    #[snafu(display("title must contain at most 200 characters"))]
    TitleTooLong,
    #[snafu(display("title must not contain control characters"))]
    TitleControlCharacter,
    #[snafu(display("sourceUrl must contain at most 2048 characters"))]
    SourceUrlTooLong,
    #[snafu(display("sourceUrl must not contain whitespace or control characters"))]
    SourceUrlCharacters,
    #[snafu(display("sourceUrl must use the http or https scheme"))]
    SourceUrlScheme,
    #[snafu(display("sourceUrl must contain a host"))]
    SourceUrlHost,
    #[snafu(display("state contains an unknown persisted reading-entry state"))]
    UnknownState,
    #[snafu(display("status must be all, queued, or completed"))]
    InvalidStatus,
    #[snafu(display("sort must be oldest or newest"))]
    InvalidOrder,
    #[snafu(display("completedEntries must be a non-negative persisted count"))]
    NegativeProgress,
}

impl DomainValidationError {
    pub const fn field(&self) -> &'static str {
        match self {
            Self::InvalidId => "id",
            Self::EmptyTitle => "title",
            Self::TitleTooLong => "title",
            Self::TitleControlCharacter => "title",
            Self::SourceUrlTooLong => "sourceUrl",
            Self::SourceUrlCharacters => "sourceUrl",
            Self::SourceUrlScheme => "sourceUrl",
            Self::SourceUrlHost => "sourceUrl",
            Self::UnknownState => "state",
            Self::InvalidStatus => "status",
            Self::InvalidOrder => "sort",
            Self::NegativeProgress => "completedEntries",
        }
    }
    pub const fn message(&self) -> &'static str {
        match self {
            Self::InvalidId => "must be a non-empty opaque identifier without whitespace",
            Self::EmptyTitle => "must not be empty",
            Self::TitleTooLong => "must contain at most 200 characters",
            Self::TitleControlCharacter => "must not contain control characters",
            Self::SourceUrlTooLong => "must contain at most 2048 characters",
            Self::SourceUrlCharacters => "must not contain whitespace or control characters",
            Self::SourceUrlScheme => "must use the http or https scheme",
            Self::SourceUrlHost => "must contain a host",
            Self::UnknownState => "contains an unknown persisted reading-entry state",
            Self::InvalidStatus => "must be all, queued, or completed",
            Self::InvalidOrder => "must be oldest or newest",
            Self::NegativeProgress => "must be a non-negative persisted count",
        }
    }
    pub const fn code(&self) -> &'static str {
        match self {
            Self::InvalidId => "invalid_id",
            Self::EmptyTitle => "empty",
            Self::TitleTooLong => "too_long",
            Self::TitleControlCharacter => "control_character",
            Self::SourceUrlTooLong => "too_long",
            Self::SourceUrlCharacters => "invalid_character",
            Self::SourceUrlScheme => "unsupported_scheme",
            Self::SourceUrlHost => "missing_host",
            Self::UnknownState => "unknown_state",
            Self::InvalidStatus => "invalid_status",
            Self::InvalidOrder => "invalid_order",
            Self::NegativeProgress => "negative_count",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ReadingEntry, ReadingEntryId, ReadingEntryOrder, ReadingEntryState,
        ReadingEntryStatusFilter, ReadingEntryTitle, ReadingProgress, SourceUrl,
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
    fn reading_progress_rejects_corrupt_negative_derived_state() {
        assert_eq!(
            ReadingProgress::restore(2)
                .expect("valid progress")
                .completed_entries(),
            2
        );
        assert!(ReadingProgress::restore(-1).is_err());
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
