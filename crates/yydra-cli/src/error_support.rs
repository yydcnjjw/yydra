// SPDX-License-Identifier: MIT OR Apache-2.0
//! Shared derive boilerplate; each CLI module still owns its classification and causes.
macro_rules! module_errors {
    ($namespace:literal, [$($kind:ident => $code:literal),+ $(,)?], [$($module_variant:ident => $module_source:ty),* $(,)?]) => {
        pub(crate) type Result<T> = std::result::Result<T, Error>;
        #[derive(Clone, Copy, Debug, Eq, PartialEq)]
        pub(crate) enum ErrorKind { $($kind,)+ MissingValue }
        impl ErrorKind {
            fn code(self) -> &'static str {
                match self { $(Self::$kind => $code,)+ Self::MissingValue => concat!($namespace, "_MISSING_VALUE") }
            }
        }
        #[derive(Debug, snafu::Snafu)]
        #[snafu(source(from(Failure, Box::new)))]
        pub(crate) struct Error(Box<Failure>);
        #[derive(Debug, snafu::Snafu)]
        enum Failure {
            #[snafu(display("{detail}"))]
            Rejected { kind: ErrorKind, detail: String },
            #[snafu(display("{operation}"))]
            Operation { operation: String, source: Cause, backtrace: Option<snafu::Backtrace> },
            // Only orchestration modules need to aggregate cleanup failures.
            #[allow(dead_code)]
            #[snafu(display("operation and cleanup both failed"))]
            Cleanup { source: Box<Error>, cleanup: Box<Error> },
        }
        #[derive(Debug, snafu::Snafu)]
        #[snafu(context(suffix(Cause)))]
        enum Cause {
            #[snafu(context(false), display("SPDX license expression is invalid"))]
            License { source: spdx::ParseError },
            #[snafu(context(false), display("file or process I/O failed"))]
            Io { source: std::io::Error },
            #[snafu(context(false), display("JSON document is invalid"))]
            Json { source: serde_json::Error },
            #[snafu(context(false), display("TOML document is invalid"))]
            Toml { source: toml::de::Error },
            #[snafu(context(false), display("TOML serialization failed"))]
            TomlEncode { source: toml::ser::Error },
            #[snafu(context(false), display("version is invalid"))]
            Version { source: semver::Error },
            #[snafu(context(false), display("signal handler installation failed"))]
            Signal { source: ctrlc::Error },
            #[snafu(context(false), display("text is not UTF-8"))]
            Utf8 { source: std::str::Utf8Error },
            #[snafu(context(false), display("integer is invalid"))]
            Integer { source: std::num::ParseIntError },
            #[snafu(context(false), display("integer is out of range"))]
            IntegerRange { source: std::num::TryFromIntError },
            #[snafu(context(false), display("path is outside its root"))]
            Path { source: std::path::StripPrefixError },
            #[cfg(unix)]
            #[snafu(context(false), display("process group operation failed"))]
            Unix { source: nix::errno::Errno },
            #[snafu(display("nested operation failed"))]
            Local { source: Box<Error> },
            $(
            #[snafu(context(false), display("module operation failed"))]
            $module_variant { #[snafu(source(from($module_source, Box::new)))] source: Box<$module_source> },
            )*

        }
        impl Error {
            pub(crate) fn rejected(kind: ErrorKind, detail: impl Into<String>) -> Self {
                Failure::Rejected { kind, detail: detail.into() }.into()
            }
            fn context(operation: impl Into<String>, source: impl Into<Cause>) -> Self {
                use snafu::GenerateImplicitData;
                Failure::Operation { operation: operation.into(), source: source.into(), backtrace: Option::<snafu::Backtrace>::generate() }.into()
            }
            #[allow(dead_code)]
            pub(crate) fn with_cleanup(self, cleanup: Self) -> Self {
                Failure::Cleanup { source: Box::new(self), cleanup: Box::new(cleanup) }.into()
            }
            pub(crate) fn code(&self) -> &'static str {
                match &*self.0 {
                    Failure::Rejected { kind, .. } => kind.code(),
                    Failure::Operation { source, .. } => source.code(),
                    Failure::Cleanup { .. } => concat!($namespace, "_CLEANUP_FAILED"),
                }
            }
            /// Only Yydra-authored context is rendered. External causes stay inspectable.
            pub(crate) fn report(&self) -> String {
                match &*self.0 {
                    Failure::Rejected { detail, .. } => detail.clone(),
                    Failure::Operation { operation, source, .. } => format!("{operation}: {}", source.report()),
                    Failure::Cleanup { source, cleanup } => format!("original: {}; cleanup: {}", source.report(), cleanup.report()),
                }
            }
        }
        impl Cause {
            fn code(&self) -> &'static str {
                match self {
                    Self::Io { source } => match source.kind() {
                        std::io::ErrorKind::NotFound => "IO_NOT_FOUND",
                        std::io::ErrorKind::PermissionDenied => "IO_PERMISSION_DENIED",
                        std::io::ErrorKind::TimedOut => "IO_TIMEOUT",
                        _ => "IO_FAILED",
                    },
                    Self::License { .. } => "SPDX_INVALID",
                    Self::Json { .. } => "JSON_INVALID",
                    Self::Toml { .. } => "TOML_INVALID",
                    Self::TomlEncode { .. } => "TOML_ENCODE_FAILED",
                    Self::Version { .. } => "VERSION_INVALID",
                    Self::Signal { .. } => "SIGNAL_HANDLER_FAILED",
                    Self::Utf8 { .. } => "UTF8_INVALID",
                    Self::Integer { .. } => "INTEGER_INVALID",
                    Self::IntegerRange { .. } => "INTEGER_OUT_OF_RANGE",
                    Self::Path { .. } => "PATH_OUTSIDE_ROOT",
                    #[cfg(unix)] Self::Unix { .. } => "PROCESS_GROUP_FAILED",
                    Self::Local { source } => source.code(),
                    $(Self::$module_variant { source } => source.code(),)*
                }
            }
            fn report(&self) -> String {
                match self {
                    Self::Local { source } => source.report(),
                    $(Self::$module_variant { source } => source.report(),)*
                    _ => self.to_string(),
                }
            }
        }
        impl From<Error> for Cause { fn from(source: Error) -> Self { Self::Local { source: Box::new(source) } } }
        pub(crate) trait Context<T> {
            fn context(self, operation: impl Into<String>) -> Result<T>;
            #[allow(dead_code)]
            fn with_context(self, operation: impl FnOnce() -> String) -> Result<T>;
        }
        impl<T, E: Into<Cause>> Context<T> for std::result::Result<T, E> {
            fn context(self, operation: impl Into<String>) -> Result<T> { self.map_err(|source| Error::context(operation, source)) }
            fn with_context(self, operation: impl FnOnce() -> String) -> Result<T> { self.map_err(|source| Error::context(operation(), source)) }
        }
        impl<T> Context<T> for Option<T> {
            fn context(self, operation: impl Into<String>) -> Result<T> { self.ok_or_else(|| Error::rejected(ErrorKind::MissingValue, operation)) }
            fn with_context(self, operation: impl FnOnce() -> String) -> Result<T> { self.ok_or_else(|| Error::rejected(ErrorKind::MissingValue, operation())) }
        }
        error_sources!(std::io::Error, serde_json::Error, toml::de::Error, toml::ser::Error,
            semver::Error, ctrlc::Error, std::str::Utf8Error, std::num::ParseIntError,
            std::num::TryFromIntError, std::path::StripPrefixError);
        #[cfg(unix)] error_sources!(nix::errno::Errno);
        $(error_sources!($module_source);)*

    };
}
macro_rules! error_sources {
    ($($source:ty),+ $(,)?) => {$(
        impl From<$source> for Error {
            fn from(source: $source) -> Self { Self::context("operation failed", source) }
        }
    )+};
}

macro_rules! fail {
    ($kind:ident, $($detail:tt)*) => { return Err(Error::rejected(ErrorKind::$kind, format!($($detail)*))) };
}
macro_rules! failure {
    ($kind:ident, $($detail:tt)*) => { Error::rejected(ErrorKind::$kind, format!($($detail)*)) };
}

#[cfg(test)]
mod tests {
    use std::error::Error as _;

    #[test]
    fn typed_reason_and_owned_context_preserve_without_rendering_external_cause() {
        use crate::Context;
        let error = Err::<(), _>(std::io::Error::other("credential=private-fixture"))
            .context("read Workspace metadata")
            .unwrap_err();
        assert_eq!(error.code(), "IO_FAILED");
        assert!(error.report().contains("read Workspace metadata"));
        assert!(!error.report().contains("private-fixture"));
        let mut source = error.source();
        while let Some(cause) = source {
            if let Some(io) = cause.downcast_ref::<std::io::Error>() {
                assert!(io.to_string().contains("private-fixture"));
                return;
            }
            source = cause.source();
        }
        panic!("the concrete I/O cause must remain inspectable");
    }
}
