// SPDX-License-Identifier: MIT OR Apache-2.0
//! HTTP adapters own mappings; this library owns formatting and request correlation.
#![forbid(unsafe_code)]

use axum::{
    Json, Router,
    extract::Request,
    http::{HeaderValue, StatusCode, header},
    middleware::{self, Next},
    response::{IntoResponse, Response},
};
use serde::Serialize;
use snafu::Snafu;
use std::error::Error as StdError;
use tracing::Instrument;
use utoipa::ToSchema;

tokio::task_local! { static REQUEST_ID: String; }

#[derive(Debug, Snafu)]
#[snafu(display("could not generate a request identity"))]
struct RequestIdentityError {
    source: getrandom::Error,
}

fn new_request_id() -> Result<String, RequestIdentityError> {
    use snafu::ResultExt;
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes).context(RequestIdentitySnafu)?;
    Ok(bytes.iter().map(|byte| format!("{byte:02x}")).collect())
}

/// A failed input rule. The adapter supplies a public field name, never its value.
#[derive(Clone, Debug, Serialize, ToSchema)]
pub struct FieldViolation {
    pub field: String,
    pub code: String,
}

/// RFC 9457 error representation; `type` is the primary machine identifier.
#[derive(Clone, Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProblemDetails {
    #[serde(rename = "type")]
    #[schema(rename = "type")]
    pub type_uri: String,
    pub title: String,
    pub status: u16,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub detail: Option<String>,
    pub request_id: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub violations: Vec<FieldViolation>,
}

impl ProblemDetails {
    pub fn new(status: StatusCode, type_uri: impl Into<String>, title: impl Into<String>) -> Self {
        Self {
            type_uri: type_uri.into(),
            title: title.into(),
            status: status.as_u16(),
            detail: None,
            request_id: String::new(),
            violations: Vec::new(),
        }
    }
}

/// Safe, adapter-selected metadata. Causes remain on the typed error, not the wire.
#[derive(Clone, Debug)]
struct FailureReport {
    operation: &'static str,
    code: &'static str,
    source_count: usize,
    context: String,
    backtrace: Option<String>,
}

pub struct ProblemResponse {
    body: ProblemDetails,
    report: Option<FailureReport>,
}

impl ProblemResponse {
    pub fn new(body: ProblemDetails) -> Self {
        Self { body, report: None }
    }

    /// `context` must be constructed from safe typed fields, not external Display/Debug.
    pub fn technical<E: StdError + snafu::ErrorCompat + 'static>(
        mut self,
        operation: &'static str,
        code: &'static str,
        context: impl Into<String>,
        error: &E,
    ) -> Self {
        let mut source = error.source();
        let mut source_count = 0;
        // Defensive bound for third-party Error implementations with cyclic sources.
        while let Some(error) = source.filter(|_| source_count < 32) {
            source_count += 1;
            source = error.source();
        }
        self.report = Some(FailureReport {
            operation,
            code,
            source_count,
            context: context.into(),
            backtrace: error.backtrace().map(ToString::to_string),
        });
        self
    }
}

impl IntoResponse for ProblemResponse {
    fn into_response(mut self) -> Response {
        let id = REQUEST_ID.try_with(Clone::clone).unwrap_or_else(|_| {
            new_request_id().unwrap_or_else(|_| "request-identity-unavailable".to_owned())
        });
        self.body.request_id = id.clone();
        let status =
            StatusCode::from_u16(self.body.status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
        if let Some(report) = self.report {
            tracing::error!(request_id = %id, operation = report.operation, code = report.code,
                source_count = report.source_count, context = %report.context, backtrace = ?report.backtrace, "request failed");
        }
        let mut response = (
            status,
            [
                (header::CONTENT_TYPE, "application/problem+json"),
                (header::CACHE_CONTROL, "no-store"),
            ],
            Json(self.body),
        )
            .into_response();
        response.headers_mut().insert(
            "x-request-id",
            HeaderValue::from_str(&id).expect("generated request ID"),
        );
        if status == StatusCode::UNAUTHORIZED {
            response.headers_mut().insert(
                header::WWW_AUTHENTICATE,
                HeaderValue::from_static("Bearer realm=\"yydra-product\""),
            );
        }
        response
    }
}

/// Apply once, outside authentication/session middleware, after all routes are assembled.
pub fn request_context(router: Router) -> Router {
    router.layer(middleware::from_fn(correlate))
}

async fn correlate(request: Request, next: Next) -> Response {
    let id = match new_request_id() {
        Ok(id) => id,
        Err(error) => {
            return ProblemResponse::new(ProblemDetails::new(
                StatusCode::INTERNAL_SERVER_ERROR,
                "https://yydra.dev/problems/internal",
                "Internal service failure",
            ))
            .technical(
                "request.identity",
                "REQUEST_ID_UNAVAILABLE",
                "random source unavailable",
                &error,
            )
            .into_response();
        }
    };
    let span = tracing::info_span!("http.request", request_id = %id);
    let mut response = REQUEST_ID
        .scope(id.clone(), next.run(request).instrument(span))
        .await;
    response.headers_mut().insert(
        "x-request-id",
        HeaderValue::from_str(&id).expect("generated request ID"),
    );
    response
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        body::{Body, to_bytes},
        routing::get,
    };
    use tower::ServiceExt;

    #[derive(Clone, Default)]
    struct Logs(std::sync::Arc<std::sync::Mutex<Vec<u8>>>);
    impl std::io::Write for Logs {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    #[tokio::test]
    async fn one_safe_report_retains_correlation_without_formatting_external_causes() {
        #[derive(Debug, Snafu)]
        #[snafu(display("database unavailable"))]
        struct Failure {
            source: std::io::Error,
        }
        let error = Failure {
            source: std::io::Error::other("password=secret-user-value"),
        };
        let logs = Logs::default();
        let writer = logs.clone();
        let subscriber = tracing_subscriber::fmt()
            .without_time()
            .with_ansi(false)
            .with_writer(move || writer.clone())
            .finish();
        let response = tracing::subscriber::with_default(subscriber, || {
            ProblemResponse::new(ProblemDetails::new(
                StatusCode::INTERNAL_SERVER_ERROR,
                "https://yydra.dev/problems/internal",
                "Internal service failure",
            ))
            .technical(
                "reading_queue.create",
                "DATABASE_IO",
                "insert entry",
                &error,
            )
            .into_response()
        });
        let id = response.headers()["x-request-id"]
            .to_str()
            .unwrap()
            .to_owned();
        let body = to_bytes(response.into_body(), 4096).await.unwrap();
        let body = String::from_utf8(body.to_vec()).unwrap();
        let logs = String::from_utf8(logs.0.lock().unwrap().clone()).unwrap();
        assert_eq!(logs.matches("request failed").count(), 1);
        assert!(logs.contains(&id));
        assert!(body.contains(&id));
        assert!(logs.contains("DATABASE_IO"));
        assert!(logs.contains("source_count=1"));
        assert!(!logs.contains("secret-user-value"));
        assert!(!body.contains("secret-user-value"));
        assert!(
            error
                .source()
                .unwrap()
                .to_string()
                .contains("secret-user-value")
        );
    }

    #[tokio::test]
    async fn request_identity_covers_errors_and_rejects_client_identity() {
        let app = request_context(Router::new().route(
            "/",
            get(|| async {
                ProblemResponse::new(ProblemDetails::new(
                    StatusCode::UNAUTHORIZED,
                    "https://yydra.dev/problems/authentication-required",
                    "Authentication required",
                ))
            }),
        ));
        let mut ids = Vec::new();
        for _ in 0..2 {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .uri("/")
                        .header("x-request-id", "client-controlled")
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
            assert!(response.headers().contains_key(header::WWW_AUTHENTICATE));
            let id = response.headers()["x-request-id"]
                .to_str()
                .unwrap()
                .to_owned();
            let body: serde_json::Value =
                serde_json::from_slice(&to_bytes(response.into_body(), 4096).await.unwrap())
                    .unwrap();
            assert_eq!(body["requestId"], id);
            assert_eq!(id.len(), 32);
            assert_ne!(id, "client-controlled");
            ids.push(id);
        }
        assert_ne!(ids[0], ids[1]);
    }
}
