// SPDX-License-Identifier: MIT OR Apache-2.0
use snafu::{Backtrace, GenerateImplicitData, Snafu};
use std::{error::Error as StdError, fmt};

type ExternalError = Box<dyn StdError + Send + Sync>;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum ErrorKind {
    InvalidRequest,
    AuthenticationRequired,
    CsrfRejected,
    Database,
    SessionUnavailable,
    Configuration,
    ProviderUnavailable,
}

#[derive(Snafu)]
#[snafu(source(from(exact)))]
pub struct AuthError(InnerError);
impl fmt::Debug for AuthError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AuthError")
            .field("kind", &self.kind())
            .field("operation", &self.operation())
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Snafu)]
enum InnerError {
    #[snafu(display("invalid authentication request"))]
    Invalid,
    #[snafu(display("authentication is required"))]
    Unauthorized,
    #[snafu(display("CSRF verification failed"))]
    Forbidden,
    #[snafu(display("{operation}: authentication database failed"))]
    Database {
        operation: &'static str,
        source: sqlx::Error,
        backtrace: Option<Backtrace>,
    },
    #[snafu(display("session state is unavailable"))]
    SessionState,
    #[snafu(display("authentication is not configured"))]
    ConfigurationMissing,
    #[snafu(display("provider returned an invalid identity"))]
    ProviderIdentity,
    #[snafu(display("{operation}: session service unavailable"))]
    Session {
        operation: &'static str,
        source: ExternalError,
        backtrace: Option<Backtrace>,
    },
    #[snafu(display("authentication configuration is invalid"))]
    Configuration { source: ExternalError },
    #[snafu(display("{operation}: identity provider unavailable"))]
    Provider {
        operation: &'static str,
        source: ExternalError,
        backtrace: Option<Backtrace>,
    },
}

impl AuthError {
    pub fn kind(&self) -> ErrorKind {
        match self.0 {
            InnerError::Invalid => ErrorKind::InvalidRequest,
            InnerError::Unauthorized => ErrorKind::AuthenticationRequired,
            InnerError::Forbidden => ErrorKind::CsrfRejected,
            InnerError::Database { .. } => ErrorKind::Database,
            InnerError::Session { .. } | InnerError::SessionState => ErrorKind::SessionUnavailable,
            InnerError::Configuration { .. } | InnerError::ConfigurationMissing => {
                ErrorKind::Configuration
            }
            InnerError::Provider { .. } | InnerError::ProviderIdentity => {
                ErrorKind::ProviderUnavailable
            }
        }
    }
    pub fn operation(&self) -> &'static str {
        match &self.0 {
            InnerError::Database { operation, .. }
            | InnerError::Session { operation, .. }
            | InnerError::Provider { operation, .. } => operation,
            InnerError::Configuration { .. } => "authentication.configure",
            _ => "authentication.validate",
        }
    }
    pub fn diagnostic_code(&self) -> &'static str {
        match self.kind() {
            ErrorKind::InvalidRequest => "AUTH_INVALID_REQUEST",
            ErrorKind::AuthenticationRequired => "AUTH_REQUIRED",
            ErrorKind::CsrfRejected => "AUTH_CSRF_REJECTED",
            ErrorKind::Database => "AUTH_DATABASE_FAILED",
            ErrorKind::SessionUnavailable => "AUTH_SESSION_UNAVAILABLE",
            ErrorKind::Configuration => "AUTH_CONFIGURATION_INVALID",
            ErrorKind::ProviderUnavailable => "AUTH_PROVIDER_UNAVAILABLE",
        }
    }
    pub(crate) fn invalid() -> Self {
        InnerError::Invalid.into()
    }
    pub(crate) fn unauthorized() -> Self {
        InnerError::Unauthorized.into()
    }
    pub(crate) fn forbidden() -> Self {
        InnerError::Forbidden.into()
    }
    pub(crate) fn configuration() -> Self {
        InnerError::ConfigurationMissing.into()
    }
    pub(crate) fn configuration_source(source: impl StdError + Send + Sync + 'static) -> Self {
        InnerError::Configuration {
            source: Box::new(source),
        }
        .into()
    }
    pub(crate) fn unavailable() -> Self {
        InnerError::SessionState.into()
    }
    pub(crate) fn session(
        operation: &'static str,
        source: impl StdError + Send + Sync + 'static,
    ) -> Self {
        InnerError::Session {
            operation,
            source: Box::new(source),
            backtrace: Option::<Backtrace>::generate(),
        }
        .into()
    }
    pub(crate) fn provider() -> Self {
        InnerError::ProviderIdentity.into()
    }
    pub(crate) fn provider_source(
        operation: &'static str,
        source: impl StdError + Send + Sync + 'static,
    ) -> Self {
        InnerError::Provider {
            operation,
            source: Box::new(source),
            backtrace: Option::<Backtrace>::generate(),
        }
        .into()
    }
}
impl From<sqlx::Error> for AuthError {
    fn from(source: sqlx::Error) -> Self {
        InnerError::Database {
            operation: "authentication.database",
            source,
            backtrace: Option::<Backtrace>::generate(),
        }
        .into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sources_survive_without_leaking_in_public_formatting() {
        let error =
            AuthError::provider_source("provider.token", std::io::Error::other("token=TOP_SECRET"));
        assert_eq!(error.kind(), ErrorKind::ProviderUnavailable);
        assert!(error.source().is_some());
        assert!(!format!("{error:?} {error}").contains("TOP_SECRET"));
        let database = AuthError::from(sqlx::Error::PoolClosed);
        assert_eq!(database.kind(), ErrorKind::Database);
        assert!(database.source().is_some());
    }
}
