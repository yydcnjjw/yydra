// SPDX-License-Identifier: MIT OR Apache-2.0

#![forbid(unsafe_code)]

use std::sync::Arc;

use axum::extract::State;
use axum::extract::rejection::JsonRejection;
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use product_application::{
    CreateReadingEntryCommand, CreateReadingEntryError, HealthService, ReadingQueueApplication,
    ReadingQueueEntry, ReadingQueueEntryState as ApplicationReadingQueueEntryState,
    ReadingQueueService,
};
use serde::{Deserialize, Serialize};
use utoipa::openapi::schema::{AdditionalProperties, Schema};
use utoipa::openapi::{Info, OpenApi, Paths, RefOr};
use utoipa::{PartialSchema, ToSchema};
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct HealthResponse {
    status: &'static str,
    database: String,
}

/// Cross-runtime wire conventions exercised by the generated client fixture.
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct FrameworkContractProfile {
    /// Opaque identifier; consumers must not infer storage semantics.
    opaque_id: String,
    /// RFC 3339 UTC timestamp.
    #[schema(format = DateTime, pattern = r"Z$")]
    occurred_at: String,
    /// JavaScript-safe integer.
    #[schema(maximum = 4_294_967_295_u64)]
    safe_count: u32,
    /// Exact decimal represented as a string.
    #[schema(pattern = r"^-?(0|[1-9][0-9]*)(\.[0-9]+)?$")]
    exact_amount: String,
    /// Collections are arrays and never null.
    items: Vec<String>,
    /// Required and explicitly nullable response field.
    #[schema(required, nullable = true)]
    nullable_note: Option<String>,
    /// Optional but non-null response field.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    optional_note: Option<String>,
}

/// Dedicated create shape retained separately from response and patch shapes.
#[derive(Debug, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FrameworkContractCreate {
    #[schema(pattern = r"^-?(0|[1-9][0-9]*)(\.[0-9]+)?$")]
    pub exact_amount: String,
    #[schema(nullable = false)]
    pub optional_note: Option<String>,
}

/// Dedicated patch shape; absent fields mean unchanged.
#[derive(Debug, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FrameworkContractPatch {
    #[schema(nullable = true)]
    pub nullable_note: Option<String>,
}

/// RFC 9457 Problem Details shared by declared public failures.
#[derive(Debug, Serialize, ToSchema)]
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
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub trace_id: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreateReadingEntryRequest {
    pub title: String,
    pub source_url: String,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum ReadingQueueEntryState {
    Queued,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ReadingQueueEntryResponse {
    /// Opaque Product Domain identifier.
    pub id: String,
    pub title: String,
    pub source_url: String,
    pub state: ReadingQueueEntryState,
}

impl From<ReadingQueueEntry> for ReadingQueueEntryResponse {
    fn from(entry: ReadingQueueEntry) -> Self {
        let state = match entry.state {
            ApplicationReadingQueueEntryState::Queued => ReadingQueueEntryState::Queued,
        };
        Self {
            id: entry.id,
            title: entry.title,
            source_url: entry.source_url,
            state,
        }
    }
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ReadingQueueResponse {
    pub entries: Vec<ReadingQueueEntryResponse>,
}

#[derive(Clone)]
pub struct ReadingQueueHttpState {
    application: Arc<dyn ReadingQueueApplication>,
}

impl ReadingQueueHttpState {
    pub fn new(application: impl ReadingQueueApplication + 'static) -> Self {
        Self {
            application: Arc::new(application),
        }
    }
}

struct ProblemResponse {
    status: StatusCode,
    body: ProblemDetails,
}

impl ProblemResponse {
    fn invalid(detail: String) -> Self {
        Self {
            status: StatusCode::UNPROCESSABLE_ENTITY,
            body: ProblemDetails {
                type_uri: "https://yydra.dev/problems/invalid-reading-entry".to_owned(),
                title: "Invalid reading entry".to_owned(),
                status: StatusCode::UNPROCESSABLE_ENTITY.as_u16(),
                detail: Some(detail),
                trace_id: None,
            },
        }
    }

    fn internal() -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            body: ProblemDetails {
                type_uri: "https://yydra.dev/problems/internal".to_owned(),
                title: "Internal service failure".to_owned(),
                status: StatusCode::INTERNAL_SERVER_ERROR.as_u16(),
                detail: None,
                trace_id: None,
            },
        }
    }
}

impl IntoResponse for ProblemResponse {
    fn into_response(self) -> Response {
        (
            self.status,
            [(header::CONTENT_TYPE, "application/problem+json")],
            Json(self.body),
        )
            .into_response()
    }
}

async fn health(State(service): State<HealthService>) -> Result<Json<HealthResponse>, StatusCode> {
    let status = service
        .check()
        .await
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    Ok(Json(HealthResponse {
        status: status.status,
        database: status.database,
    }))
}

#[utoipa::path(
    get,
    path = "/api/v1/framework-contract",
    operation_id = "getFrameworkContractProfile",
    tag = "framework",
    responses(
        (status = 200, description = "Wire-contract fixture", body = FrameworkContractProfile, content_type = "application/json"),
        (status = 500, description = "Contract-valid server failure", body = ProblemDetails, content_type = "application/problem+json")
    )
)]
async fn get_framework_contract_profile() -> Json<FrameworkContractProfile> {
    Json(FrameworkContractProfile {
        opaque_id: "framework-contract-v1".to_owned(),
        occurred_at: "2000-01-01T00:00:00Z".to_owned(),
        safe_count: 1,
        exact_amount: "0.01".to_owned(),
        items: Vec::new(),
        nullable_note: None,
        optional_note: None,
    })
}

#[utoipa::path(
    post,
    path = "/api/v1/reading-queue/entries",
    operation_id = "createReadingQueueEntry",
    tag = "readingQueue",
    request_body(content = CreateReadingEntryRequest, content_type = "application/json"),
    responses(
        (status = 201, description = "Reading entry created", body = ReadingQueueEntryResponse, content_type = "application/json"),
        (status = 422, description = "Invalid reading entry", body = ProblemDetails, content_type = "application/problem+json"),
        (status = 500, description = "Storage failure", body = ProblemDetails, content_type = "application/problem+json")
    )
)]
async fn create_reading_queue_entry(
    State(state): State<ReadingQueueHttpState>,
    payload: Result<Json<CreateReadingEntryRequest>, JsonRejection>,
) -> Result<(StatusCode, Json<ReadingQueueEntryResponse>), ProblemResponse> {
    let Json(payload) = payload.map_err(|error| ProblemResponse::invalid(error.body_text()))?;
    let entry = state
        .application
        .create(CreateReadingEntryCommand {
            title: payload.title,
            source_url: payload.source_url,
        })
        .await
        .map_err(|error| match error {
            CreateReadingEntryError::InvalidInput { field, message } => {
                ProblemResponse::invalid(format!("{field} {message}"))
            }
            CreateReadingEntryError::Storage(_) => ProblemResponse::internal(),
        })?;
    Ok((StatusCode::CREATED, Json(entry.into())))
}

#[utoipa::path(
    get,
    path = "/api/v1/reading-queue/entries",
    operation_id = "listReadingQueueEntries",
    tag = "readingQueue",
    responses(
        (status = 200, description = "Reading queue", body = ReadingQueueResponse, content_type = "application/json"),
        (status = 500, description = "Storage failure", body = ProblemDetails, content_type = "application/problem+json")
    )
)]
async fn list_reading_queue_entries(
    State(state): State<ReadingQueueHttpState>,
) -> Result<Json<ReadingQueueResponse>, ProblemResponse> {
    let entries = state
        .application
        .list()
        .await
        .map_err(|_| ProblemResponse::internal())?;
    Ok(Json(ReadingQueueResponse {
        entries: entries.into_iter().map(Into::into).collect(),
    }))
}

/// The only registration seam for routes consumed by the Generated Client.
pub fn public_routes() -> OpenApiRouter<ReadingQueueHttpState> {
    let openapi = OpenApi::new(Info::new("Yydra Product Public API", "1.0.0"), Paths::new());
    let mut router = OpenApiRouter::with_openapi(openapi)
        .routes(routes!(get_framework_contract_profile))
        .routes(routes!(
            create_reading_queue_entry,
            list_reading_queue_entries
        ));
    let schemas = &mut router
        .get_openapi_mut()
        .components
        .get_or_insert_default()
        .schemas;
    schemas.insert(
        "FrameworkContractCreate".to_owned(),
        FrameworkContractCreate::schema(),
    );
    schemas.insert(
        "FrameworkContractPatch".to_owned(),
        FrameworkContractPatch::schema(),
    );
    for response_schema in [
        "FrameworkContractProfile",
        "ProblemDetails",
        "ReadingQueueEntryResponse",
        "ReadingQueueResponse",
    ] {
        let Some(RefOr::T(Schema::Object(schema))) = schemas.get_mut(response_schema) else {
            panic!("{response_schema} must be a collected object schema");
        };
        schema.additional_properties = Some(Box::new(AdditionalProperties::FreeForm(true)));
    }
    router
}

/// Deterministic, normalized Public API Contract derived from `public_routes`.
pub fn normalized_openapi_json() -> Result<String, serde_json::Error> {
    let openapi = public_routes().into_openapi();
    let value = serde_json::to_value(openapi)?;
    let mut bytes = serde_json::to_vec_pretty(&value)?;
    bytes.push(b'\n');
    String::from_utf8(bytes).map_err(|error| {
        serde_json::Error::io(std::io::Error::new(std::io::ErrorKind::InvalidData, error))
    })
}

pub fn router(service: HealthService, reading_queue: ReadingQueueService) -> Router {
    let public_router: Router = public_routes()
        .with_state(ReadingQueueHttpState::new(reading_queue))
        .into();
    Router::new()
        .route("/health", get(health))
        .merge(public_router.with_state::<HealthService>(()))
        .with_state(service)
}
