// SPDX-License-Identifier: MIT OR Apache-2.0

#![forbid(unsafe_code)]

use std::sync::Arc;

use axum::extract::rejection::{JsonRejection, QueryRejection};
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use product_application::{
    ChangeReadingEntryStateCommand, ChangeReadingEntryStateError, CreateReadingEntryCommand,
    CreateReadingEntryError, HealthService, ListReadingEntriesError, ListReadingEntriesQuery,
    ReadingQueueApplication, ReadingQueueEntry,
    ReadingQueueEntryState as ApplicationReadingQueueEntryState, ReadingQueueService,
};
use serde::{Deserialize, Serialize};
use utoipa::openapi::schema::{AdditionalProperties, Schema};
use utoipa::openapi::{Info, OpenApi, Paths, RefOr};
use utoipa::{IntoParams, PartialSchema, ToSchema};
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

#[derive(Clone, Copy, Debug, Deserialize, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum ReadingQueueEntryState {
    Queued,
    Completed,
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
            ApplicationReadingQueueEntryState::Completed => ReadingQueueEntryState::Completed,
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
    #[schema(required, nullable = true)]
    pub next_cursor: Option<String>,
}

#[derive(Clone, Copy, Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum ReadingQueueStatusFilter {
    All,
    Queued,
    Completed,
}

impl ReadingQueueStatusFilter {
    fn as_str(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::Queued => "queued",
            Self::Completed => "completed",
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum ReadingQueueSort {
    Oldest,
    Newest,
}

impl ReadingQueueSort {
    fn as_str(self) -> &'static str {
        match self {
            Self::Oldest => "oldest",
            Self::Newest => "newest",
        }
    }
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ListReadingQueueEntriesQuery {
    pub status: Option<ReadingQueueStatusFilter>,
    pub sort: Option<ReadingQueueSort>,
    #[param(minimum = 1, maximum = 50)]
    pub limit: Option<u16>,
    #[param(max_length = 2048)]
    pub cursor: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ChangeReadingEntryStateRequest {
    pub state: ReadingQueueEntryState,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct FrameworkProtectedContract {
    pub access: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RouteAccess {
    Anonymous,
    Protected,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AuthenticationDecision {
    Authorized,
    MissingCredentials,
    Forbidden,
}

pub trait Authentication: Send + Sync {
    fn authenticate(&self, headers: &HeaderMap) -> AuthenticationDecision;
}

#[derive(Clone)]
pub struct BearerAuthentication {
    authorized: Arc<str>,
    forbidden: Arc<str>,
}

impl BearerAuthentication {
    pub fn new(
        authorized: impl Into<String>,
        forbidden: impl Into<String>,
    ) -> Result<Self, AuthenticationConfigurationError> {
        let authorized = authorized.into();
        let forbidden = forbidden.into();
        if authorized.is_empty()
            || authorized.chars().any(char::is_whitespace)
            || forbidden.is_empty()
            || forbidden.chars().any(char::is_whitespace)
            || authorized == forbidden
        {
            return Err(AuthenticationConfigurationError);
        }
        Ok(Self {
            authorized: Arc::from(authorized),
            forbidden: Arc::from(forbidden),
        })
    }
}

impl Authentication for BearerAuthentication {
    fn authenticate(&self, headers: &HeaderMap) -> AuthenticationDecision {
        let Some(value) = headers.get(header::AUTHORIZATION) else {
            return AuthenticationDecision::MissingCredentials;
        };
        let Ok(value) = value.to_str() else {
            return AuthenticationDecision::MissingCredentials;
        };
        let Some(token) = value.strip_prefix("Bearer ") else {
            return AuthenticationDecision::MissingCredentials;
        };
        if token == self.authorized.as_ref() {
            AuthenticationDecision::Authorized
        } else if token == self.forbidden.as_ref() {
            AuthenticationDecision::Forbidden
        } else {
            AuthenticationDecision::MissingCredentials
        }
    }
}

#[derive(Debug)]
pub struct AuthenticationConfigurationError;

impl std::fmt::Display for AuthenticationConfigurationError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(
            "authentication fixture credentials must be distinct and non-empty without whitespace",
        )
    }
}

impl std::error::Error for AuthenticationConfigurationError {}

const READING_QUEUE_ACCESS: RouteAccess = RouteAccess::Anonymous;
const FRAMEWORK_PROTECTED_ACCESS: RouteAccess = RouteAccess::Protected;

#[derive(Clone)]
pub struct ReadingQueueHttpState {
    application: Arc<dyn ReadingQueueApplication>,
    authentication: Arc<dyn Authentication>,
}

impl ReadingQueueHttpState {
    pub fn new(
        application: impl ReadingQueueApplication + 'static,
        authentication: impl Authentication + 'static,
    ) -> Self {
        Self {
            application: Arc::new(application),
            authentication: Arc::new(authentication),
        }
    }

    fn authorize(
        &self,
        access: RouteAccess,
        headers: &HeaderMap,
    ) -> Result<AuthorizationContext, ProblemResponse> {
        if access == RouteAccess::Anonymous {
            return Ok(AuthorizationContext {
                cursor_scope: "anonymous",
            });
        }
        match self.authentication.authenticate(headers) {
            AuthenticationDecision::Authorized => Ok(AuthorizationContext {
                cursor_scope: "protected-contract",
            }),
            AuthenticationDecision::MissingCredentials => Err(ProblemResponse::unauthorized()),
            AuthenticationDecision::Forbidden => Err(ProblemResponse::forbidden()),
        }
    }
}

struct AuthorizationContext {
    cursor_scope: &'static str,
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

    fn invalid_json() -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            body: ProblemDetails {
                type_uri: "https://yydra.dev/problems/invalid-request-body".to_owned(),
                title: "Invalid request body".to_owned(),
                status: StatusCode::BAD_REQUEST.as_u16(),
                detail: None,
                trace_id: None,
            },
        }
    }

    fn invalid_reading_queue_query() -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            body: ProblemDetails {
                type_uri: "https://yydra.dev/problems/invalid-reading-queue-query".to_owned(),
                title: "Invalid Reading Queue query".to_owned(),
                status: StatusCode::BAD_REQUEST.as_u16(),
                detail: None,
                trace_id: None,
            },
        }
    }

    fn invalid_reading_queue_cursor() -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            body: ProblemDetails {
                type_uri: "https://yydra.dev/problems/invalid-reading-queue-cursor".to_owned(),
                title: "Invalid Reading Queue cursor".to_owned(),
                status: StatusCode::BAD_REQUEST.as_u16(),
                detail: None,
                trace_id: None,
            },
        }
    }

    fn not_found() -> Self {
        Self {
            status: StatusCode::NOT_FOUND,
            body: ProblemDetails {
                type_uri: "https://yydra.dev/problems/reading-entry-not-found".to_owned(),
                title: "Reading entry not found".to_owned(),
                status: StatusCode::NOT_FOUND.as_u16(),
                detail: None,
                trace_id: None,
            },
        }
    }

    fn conflict() -> Self {
        Self {
            status: StatusCode::CONFLICT,
            body: ProblemDetails {
                type_uri: "https://yydra.dev/problems/reading-entry-transition-conflict".to_owned(),
                title: "Reading entry transition conflict".to_owned(),
                status: StatusCode::CONFLICT.as_u16(),
                detail: None,
                trace_id: None,
            },
        }
    }

    fn unauthorized() -> Self {
        Self {
            status: StatusCode::UNAUTHORIZED,
            body: ProblemDetails {
                type_uri: "https://yydra.dev/problems/authentication-required".to_owned(),
                title: "Authentication required".to_owned(),
                status: StatusCode::UNAUTHORIZED.as_u16(),
                detail: None,
                trace_id: None,
            },
        }
    }

    fn forbidden() -> Self {
        Self {
            status: StatusCode::FORBIDDEN,
            body: ProblemDetails {
                type_uri: "https://yydra.dev/problems/access-forbidden".to_owned(),
                title: "Access forbidden".to_owned(),
                status: StatusCode::FORBIDDEN.as_u16(),
                detail: None,
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
        let unauthorized = self.status == StatusCode::UNAUTHORIZED;
        let mut response = (
            self.status,
            [(header::CONTENT_TYPE, "application/problem+json")],
            Json(self.body),
        )
            .into_response();
        if unauthorized {
            response.headers_mut().insert(
                header::WWW_AUTHENTICATE,
                HeaderValue::from_static("Bearer realm=\"yydra-framework-contract\""),
            );
        }
        response
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
    get,
    path = "/api/v1/framework-auth-contract",
    operation_id = "getFrameworkProtectedContract",
    tag = "framework",
    responses(
        (status = 200, description = "Protected authentication-contract fixture", body = FrameworkProtectedContract, content_type = "application/json"),
        (status = 401, description = "Missing credentials", body = ProblemDetails, content_type = "application/problem+json"),
        (status = 403, description = "Credentials lack access", body = ProblemDetails, content_type = "application/problem+json")
    )
)]
async fn get_framework_protected_contract(
    State(state): State<ReadingQueueHttpState>,
    headers: HeaderMap,
) -> Result<Json<FrameworkProtectedContract>, ProblemResponse> {
    state.authorize(FRAMEWORK_PROTECTED_ACCESS, &headers)?;
    Ok(Json(FrameworkProtectedContract {
        access: "granted".to_owned(),
    }))
}

#[utoipa::path(
    post,
    path = "/api/v1/reading-queue/entries",
    operation_id = "createReadingQueueEntry",
    tag = "readingQueue",
    request_body(content = CreateReadingEntryRequest, content_type = "application/json"),
    responses(
        (status = 201, description = "Reading entry created", body = ReadingQueueEntryResponse, content_type = "application/json"),
        (status = 400, description = "Invalid JSON or unknown request field", body = ProblemDetails, content_type = "application/problem+json"),
        (status = 422, description = "Invalid reading entry", body = ProblemDetails, content_type = "application/problem+json"),
        (status = 500, description = "Storage failure", body = ProblemDetails, content_type = "application/problem+json")
    )
)]
async fn create_reading_queue_entry(
    State(state): State<ReadingQueueHttpState>,
    headers: HeaderMap,
    payload: Result<Json<CreateReadingEntryRequest>, JsonRejection>,
) -> Result<(StatusCode, Json<ReadingQueueEntryResponse>), ProblemResponse> {
    state.authorize(READING_QUEUE_ACCESS, &headers)?;
    let Json(payload) = payload.map_err(|_| ProblemResponse::invalid_json())?;
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
        (status = 400, description = "Invalid query or cursor", body = ProblemDetails, content_type = "application/problem+json"),
        (status = 500, description = "Storage failure", body = ProblemDetails, content_type = "application/problem+json")
    ),
    params(ListReadingQueueEntriesQuery)
)]
async fn list_reading_queue_entries(
    State(state): State<ReadingQueueHttpState>,
    headers: HeaderMap,
    query: Result<Query<ListReadingQueueEntriesQuery>, QueryRejection>,
) -> Result<Json<ReadingQueueResponse>, ProblemResponse> {
    let authorization = state.authorize(READING_QUEUE_ACCESS, &headers)?;
    let Query(query) = query.map_err(|_| ProblemResponse::invalid_reading_queue_query())?;
    let page = state
        .application
        .list(ListReadingEntriesQuery {
            status: query.status.map(|status| status.as_str().to_owned()),
            sort: query.sort.map(|sort| sort.as_str().to_owned()),
            limit: query.limit,
            cursor: query.cursor,
            authorization_scope: authorization.cursor_scope.to_owned(),
        })
        .await
        .map_err(|error| match error {
            ListReadingEntriesError::InvalidInput { .. } => {
                ProblemResponse::invalid_reading_queue_query()
            }
            ListReadingEntriesError::InvalidCursor => {
                ProblemResponse::invalid_reading_queue_cursor()
            }
            ListReadingEntriesError::Storage(_) => ProblemResponse::internal(),
        })?;
    Ok(Json(ReadingQueueResponse {
        entries: page.entries.into_iter().map(Into::into).collect(),
        next_cursor: page.next_cursor,
    }))
}

#[utoipa::path(
    patch,
    path = "/api/v1/reading-queue/entries/{entry_id}",
    operation_id = "changeReadingQueueEntryState",
    tag = "readingQueue",
    params(
        ("entry_id" = String, Path, description = "Opaque Product Domain identifier")
    ),
    request_body(content = ChangeReadingEntryStateRequest, content_type = "application/json"),
    responses(
        (status = 200, description = "Reading entry state changed", body = ReadingQueueEntryResponse, content_type = "application/json"),
        (status = 400, description = "Invalid JSON or unknown request field", body = ProblemDetails, content_type = "application/problem+json"),
        (status = 404, description = "Reading entry not found", body = ProblemDetails, content_type = "application/problem+json"),
        (status = 409, description = "Prohibited state transition", body = ProblemDetails, content_type = "application/problem+json"),
        (status = 422, description = "Invalid opaque identifier", body = ProblemDetails, content_type = "application/problem+json"),
        (status = 500, description = "Storage failure", body = ProblemDetails, content_type = "application/problem+json")
    )
)]
async fn change_reading_queue_entry_state(
    State(state): State<ReadingQueueHttpState>,
    Path(entry_id): Path<String>,
    headers: HeaderMap,
    payload: Result<Json<ChangeReadingEntryStateRequest>, JsonRejection>,
) -> Result<Json<ReadingQueueEntryResponse>, ProblemResponse> {
    state.authorize(READING_QUEUE_ACCESS, &headers)?;
    let Json(payload) = payload.map_err(|_| ProblemResponse::invalid_json())?;
    let target = match payload.state {
        ReadingQueueEntryState::Queued => ApplicationReadingQueueEntryState::Queued,
        ReadingQueueEntryState::Completed => ApplicationReadingQueueEntryState::Completed,
    };
    let entry = state
        .application
        .change(ChangeReadingEntryStateCommand {
            id: entry_id,
            target,
        })
        .await
        .map_err(|error| match error {
            ChangeReadingEntryStateError::InvalidInput { field, message } => {
                ProblemResponse::invalid(format!("{field} {message}"))
            }
            ChangeReadingEntryStateError::NotFound { .. } => ProblemResponse::not_found(),
            ChangeReadingEntryStateError::Conflict { .. } => ProblemResponse::conflict(),
            ChangeReadingEntryStateError::Storage(_) => ProblemResponse::internal(),
        })?;
    Ok(Json(entry.into()))
}

/// The only registration seam for routes consumed by the Generated Client.
pub fn public_routes() -> OpenApiRouter<ReadingQueueHttpState> {
    let openapi = OpenApi::new(Info::new("Yydra Product Public API", "1.0.0"), Paths::new());
    let mut router = OpenApiRouter::with_openapi(openapi)
        .routes(routes!(get_framework_contract_profile))
        .routes(routes!(get_framework_protected_contract))
        .routes(routes!(
            create_reading_queue_entry,
            list_reading_queue_entries
        ))
        .routes(routes!(change_reading_queue_entry_state));
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
    schemas.insert(
        "ReadingQueueStatusFilter".to_owned(),
        ReadingQueueStatusFilter::schema(),
    );
    schemas.insert("ReadingQueueSort".to_owned(), ReadingQueueSort::schema());
    for response_schema in [
        "FrameworkContractProfile",
        "FrameworkProtectedContract",
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

pub fn router(
    service: HealthService,
    reading_queue: ReadingQueueService,
    authentication: BearerAuthentication,
) -> Router {
    let public_router: Router = public_routes()
        .with_state(ReadingQueueHttpState::new(reading_queue, authentication))
        .into();
    Router::new()
        .route("/health", get(health))
        .merge(public_router.with_state::<HealthService>(()))
        .with_state(service)
}
