use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use product_application::{
    self as application, AppError, CreateReadingEntry, ReadingEntryPage, ReadingEntryView,
};
use product_domain::{ReadingStatus, ReadingTransition};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use tower_http::cors::CorsLayer;
use utoipa::{IntoParams, OpenApi, ToSchema};
use utoipa_axum::{router::OpenApiRouter, routes};

const INVALID_TRANSITION_TYPE: &str = "https://yydra.dev/problems/invalid-reading-entry-transition";
const INVALID_INPUT_TYPE: &str = "https://yydra.dev/problems/invalid-input";
const INVALID_CURSOR_TYPE: &str = "https://yydra.dev/problems/invalid-cursor";
const NOT_FOUND_TYPE: &str = "https://yydra.dev/problems/reading-entry-not-found";
const INTERNAL_TYPE: &str = "https://yydra.dev/problems/internal";

#[derive(OpenApi)]
#[openapi(
    info(title = "Reading Queue Testbed Public API", version = "0.1.0"),
    components(schemas(
        HealthResponse,
        CreateReadingEntryRequest,
        ReadingEntryResponse,
        ReadingEntryPageResponse,
        ReadingStatusResponse,
        Problem
    ))
)]
struct ApiDoc;

#[derive(Clone)]
pub struct AppState {
    pool: PgPool,
}

impl AppState {
    #[must_use]
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct HealthResponse {
    status: &'static str,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreateReadingEntryRequest {
    title: String,
    source_url: String,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ReadingEntryResponse {
    id: String,
    title: String,
    source_url: String,
    status: ReadingStatusResponse,
}

impl From<ReadingEntryView> for ReadingEntryResponse {
    fn from(value: ReadingEntryView) -> Self {
        Self {
            id: value.id,
            title: value.title,
            source_url: value.source_url,
            status: value.status.into(),
        }
    }
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ReadingEntryPageResponse {
    items: Vec<ReadingEntryResponse>,
    next_cursor: Option<String>,
}

impl From<ReadingEntryPage> for ReadingEntryPageResponse {
    fn from(value: ReadingEntryPage) -> Self {
        Self {
            items: value.items.into_iter().map(Into::into).collect(),
            next_cursor: value.next_cursor,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum ReadingStatusResponse {
    Queued,
    Completed,
}

impl From<ReadingStatus> for ReadingStatusResponse {
    fn from(value: ReadingStatus) -> Self {
        match value {
            ReadingStatus::Queued => Self::Queued,
            ReadingStatus::Completed => Self::Completed,
        }
    }
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ListReadingEntriesQuery {
    status: Option<String>,
    cursor: Option<String>,
    #[param(minimum = 1, maximum = 50)]
    limit: Option<u16>,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct Problem {
    r#type: String,
    title: String,
    status: u16,
    detail: String,
    trace_id: String,
}

#[utoipa::path(
    get,
    path = "/health",
    operation_id = "getHealth",
    responses((status = 200, description = "Workspace process is ready", body = HealthResponse))
)]
async fn health() -> Json<HealthResponse> {
    Json(HealthResponse { status: "ready" })
}

#[utoipa::path(
    post,
    path = "/reading-entries",
    operation_id = "createReadingEntry",
    request_body = CreateReadingEntryRequest,
    responses(
        (status = 201, description = "Reading entry created", body = ReadingEntryResponse),
        (status = 400, description = "Invalid input", body = Problem, content_type = "application/problem+json"),
        (status = 500, description = "Internal failure", body = Problem, content_type = "application/problem+json")
    )
)]
async fn create_entry(
    State(state): State<AppState>,
    Json(request): Json<CreateReadingEntryRequest>,
) -> Result<(StatusCode, Json<ReadingEntryResponse>), HttpError> {
    let entry = application::create_entry(
        &state.pool,
        CreateReadingEntry {
            title: request.title,
            source_url: request.source_url,
        },
    )
    .await?;
    Ok((StatusCode::CREATED, Json(entry.into())))
}

#[utoipa::path(
    get,
    path = "/reading-entries",
    operation_id = "listReadingEntries",
    params(ListReadingEntriesQuery),
    responses(
        (status = 200, description = "Reading entry page", body = ReadingEntryPageResponse),
        (status = 400, description = "Invalid filter or cursor", body = Problem, content_type = "application/problem+json"),
        (status = 500, description = "Internal failure", body = Problem, content_type = "application/problem+json")
    )
)]
async fn list_entries(
    State(state): State<AppState>,
    Query(query): Query<ListReadingEntriesQuery>,
) -> Result<Json<ReadingEntryPageResponse>, HttpError> {
    let page = application::list_entries(
        &state.pool,
        query.status.as_deref(),
        query.cursor.as_deref(),
        query.limit,
    )
    .await?;
    Ok(Json(page.into()))
}

#[utoipa::path(
    post,
    path = "/reading-entries/{id}/complete",
    operation_id = "completeReadingEntry",
    params(("id" = String, Path, description = "Opaque reading entry identifier")),
    responses(
        (status = 200, description = "Reading entry completed", body = ReadingEntryResponse),
        (status = 404, description = "Reading entry not found", body = Problem, content_type = "application/problem+json"),
        (status = 409, description = "Invalid state transition", body = Problem, content_type = "application/problem+json"),
        (status = 500, description = "Internal failure", body = Problem, content_type = "application/problem+json")
    )
)]
async fn complete_entry(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<ReadingEntryResponse>, HttpError> {
    transition(&state, &id, ReadingTransition::Complete).await
}

#[utoipa::path(
    post,
    path = "/reading-entries/{id}/reopen",
    operation_id = "reopenReadingEntry",
    params(("id" = String, Path, description = "Opaque reading entry identifier")),
    responses(
        (status = 200, description = "Reading entry reopened", body = ReadingEntryResponse),
        (status = 404, description = "Reading entry not found", body = Problem, content_type = "application/problem+json"),
        (status = 409, description = "Invalid state transition", body = Problem, content_type = "application/problem+json"),
        (status = 500, description = "Internal failure", body = Problem, content_type = "application/problem+json")
    )
)]
async fn reopen_entry(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<ReadingEntryResponse>, HttpError> {
    transition(&state, &id, ReadingTransition::Reopen).await
}

async fn transition(
    state: &AppState,
    id: &str,
    transition: ReadingTransition,
) -> Result<Json<ReadingEntryResponse>, HttpError> {
    let entry = application::transition_entry(&state.pool, id, transition).await?;
    Ok(Json(entry.into()))
}

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::with_openapi(ApiDoc::openapi())
        .routes(routes!(health))
        .routes(routes!(create_entry, list_entries))
        .routes(routes!(complete_entry))
        .routes(routes!(reopen_entry))
        .layer(CorsLayer::permissive())
}

pub fn openapi() -> utoipa::openapi::OpenApi {
    router().into_openapi()
}

struct HttpError(AppError);

impl From<AppError> for HttpError {
    fn from(value: AppError) -> Self {
        Self(value)
    }
}

impl IntoResponse for HttpError {
    fn into_response(self) -> Response {
        let (status, problem_type, title, detail) = match self.0 {
            AppError::InvalidInput(detail) => (
                StatusCode::BAD_REQUEST,
                INVALID_INPUT_TYPE,
                "Invalid input",
                detail,
            ),
            AppError::InvalidCursor => (
                StatusCode::BAD_REQUEST,
                INVALID_CURSOR_TYPE,
                "Invalid cursor",
                "The cursor is malformed or belongs to different filters".to_owned(),
            ),
            AppError::NotFound => (
                StatusCode::NOT_FOUND,
                NOT_FOUND_TYPE,
                "Reading entry not found",
                "No reading entry exists for the supplied identifier".to_owned(),
            ),
            AppError::InvalidTransition { current, attempted } => (
                StatusCode::CONFLICT,
                INVALID_TRANSITION_TYPE,
                "Invalid reading entry transition",
                format!("cannot {attempted} a {current} reading entry"),
            ),
            AppError::Persistence(error) => {
                tracing::error!(error = %error, "reading queue persistence failure");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    INTERNAL_TYPE,
                    "Internal server error",
                    "The request could not be completed".to_owned(),
                )
            }
        };
        let problem = Problem {
            r#type: problem_type.to_owned(),
            title: title.to_owned(),
            status: status.as_u16(),
            detail,
            trace_id: uuid::Uuid::now_v7().to_string(),
        };
        (
            status,
            [(header::CONTENT_TYPE, "application/problem+json")],
            Json(problem),
        )
            .into_response()
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn router_builds_without_overlapping_routes() {
        let _ = super::router();
    }
}
