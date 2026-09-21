// SPDX-License-Identifier: MIT OR Apache-2.0
use snafu::{Backtrace, GenerateImplicitData, Snafu};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum ErrorKind {
    ClientGenerationFailed,
    ClientOutputInvalid,
    ClientToolVersionInvalid,
    ClientTypecheckFailed,
    Input,
    OpenapiContentTypeInvalid,
    OpenapiDecimalInvalid,
    OpenapiExportFailed,
    OpenapiFieldNameInvalid,
    OpenapiNullabilityInvalid,
    OpenapiOperationIdInvalid,
    OpenapiProfileInvalid,
    OpenapiRequirednessInvalid,
    OpenapiSafeIntegerInvalid,
    OpenapiShapeReuseInvalid,
    OpenapiTimestampInvalid,
    OpenapiUnknownFieldPolicyInvalid,
    OpenapiWireTypeInvalid,
    Output,
    OutputPrepareFailed,
    WorkspaceInvalid,
    Io,
    Json,
    InvalidOutputPath,
}
impl ErrorKind {
    pub const fn code(self) -> &'static str {
        match self {
            Self::ClientGenerationFailed => "API_CLIENT_GENERATION_FAILED",
            Self::ClientOutputInvalid => "API_CLIENT_OUTPUT_INVALID",
            Self::ClientToolVersionInvalid => "API_CLIENT_TOOL_VERSION_INVALID",
            Self::ClientTypecheckFailed => "API_CLIENT_TYPECHECK_FAILED",
            Self::Input => "API_INPUT",
            Self::OpenapiContentTypeInvalid => "API_OPENAPI_CONTENT_TYPE_INVALID",
            Self::OpenapiDecimalInvalid => "API_OPENAPI_DECIMAL_INVALID",
            Self::OpenapiExportFailed => "API_OPENAPI_EXPORT_FAILED",
            Self::OpenapiFieldNameInvalid => "API_OPENAPI_FIELD_NAME_INVALID",
            Self::OpenapiNullabilityInvalid => "API_OPENAPI_NULLABILITY_INVALID",
            Self::OpenapiOperationIdInvalid => "API_OPENAPI_OPERATION_ID_INVALID",
            Self::OpenapiProfileInvalid => "API_OPENAPI_PROFILE_INVALID",
            Self::OpenapiRequirednessInvalid => "API_OPENAPI_REQUIREDNESS_INVALID",
            Self::OpenapiSafeIntegerInvalid => "API_OPENAPI_SAFE_INTEGER_INVALID",
            Self::OpenapiShapeReuseInvalid => "API_OPENAPI_SHAPE_REUSE_INVALID",
            Self::OpenapiTimestampInvalid => "API_OPENAPI_TIMESTAMP_INVALID",
            Self::OpenapiUnknownFieldPolicyInvalid => "API_OPENAPI_UNKNOWN_FIELD_POLICY_INVALID",
            Self::OpenapiWireTypeInvalid => "API_OPENAPI_WIRE_TYPE_INVALID",
            Self::Output => "API_OUTPUT",
            Self::OutputPrepareFailed => "API_OUTPUT_PREPARE_FAILED",
            Self::WorkspaceInvalid => "API_WORKSPACE_INVALID",
            Self::Io => "API_IO_FAILED",
            Self::Json => "API_JSON_FAILED",
            Self::InvalidOutputPath => "API_OUTPUT_PATH_INVALID",
        }
    }
}

#[derive(Debug, Snafu)]
#[snafu(source(from(exact)))]
pub struct Error(Failure);

#[derive(Debug, Snafu)]
enum Failure {
    #[snafu(display("{}: {detail}", kind.code()))]
    Input { kind: ErrorKind, detail: String },
    #[snafu(display("{}: {operation}", kind.code()))]
    Operation {
        kind: ErrorKind,
        operation: String,
        source: Cause,
        backtrace: Option<Backtrace>,
    },
    #[snafu(display("{}: {program} exited with {}; {output}", kind.code(), exit.map_or_else(|| "signal".to_owned(), |code| code.to_string())))]
    Command {
        kind: ErrorKind,
        program: String,
        exit: Option<i32>,
        output: String,
    },
}

impl Error {
    pub fn kind(&self) -> ErrorKind {
        match &self.0 {
            Failure::Input { kind, .. }
            | Failure::Operation { kind, .. }
            | Failure::Command { kind, .. } => *kind,
        }
    }
    pub fn code(&self) -> &'static str {
        self.kind().code()
    }
    pub(crate) fn input(kind: ErrorKind, detail: String) -> Self {
        Failure::Input { kind, detail }.into()
    }
    pub(crate) fn command(
        kind: ErrorKind,
        program: &str,
        exit: Option<i32>,
        output: String,
    ) -> Self {
        Failure::Command {
            kind,
            program: program.into(),
            exit,
            output,
        }
        .into()
    }
    fn operation(kind: ErrorKind, operation: String, source: impl Into<Cause>) -> Self {
        Failure::Operation {
            kind,
            operation,
            source: source.into(),
            backtrace: Option::<Backtrace>::generate(),
        }
        .into()
    }
}

pub(crate) trait BuildContext<T> {
    fn build_context(self, kind: ErrorKind, operation: impl Into<String>) -> Result<T, Error>;
}
impl<T, E: Into<Cause>> BuildContext<T> for Result<T, E> {
    fn build_context(self, kind: ErrorKind, operation: impl Into<String>) -> Result<T, Error> {
        self.map_err(|source| Error::operation(kind, operation.into(), source))
    }
}
impl<T> BuildContext<T> for Option<T> {
    fn build_context(self, kind: ErrorKind, operation: impl Into<String>) -> Result<T, Error> {
        self.ok_or_else(|| Error::input(kind, operation.into()))
    }
}
impl From<std::io::Error> for Error {
    fn from(source: std::io::Error) -> Self {
        Self::operation(ErrorKind::Io, "access generated files".into(), source)
    }
}
impl From<serde_json::Error> for Error {
    fn from(source: serde_json::Error) -> Self {
        Self::operation(ErrorKind::Json, "encode generated metadata".into(), source)
    }
}
impl From<std::path::StripPrefixError> for Error {
    fn from(source: std::path::StripPrefixError) -> Self {
        Self::operation(
            ErrorKind::InvalidOutputPath,
            "resolve generated file path".into(),
            source,
        )
    }
}

#[derive(Debug, Snafu)]
#[snafu(context(suffix(Cause)))]
enum Cause {
    #[snafu(context(false), display("file operation failed"))]
    Io { source: std::io::Error },
    #[snafu(context(false), display("JSON operation failed"))]
    Json { source: serde_json::Error },
    #[snafu(context(false), display("HTTP status is not an integer"))]
    Status { source: std::num::ParseIntError },
    #[snafu(context(false), display("output path is invalid"))]
    Path { source: std::path::StripPrefixError },
}
