// SPDX-License-Identifier: MIT OR Apache-2.0

#![forbid(unsafe_code)]

use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::get;
use axum::{Json, Router};
use product_application::HealthService;
use serde::Serialize;
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

/// The only registration seam for routes consumed by the Generated Client.
pub fn public_routes() -> OpenApiRouter {
    let openapi = OpenApi::new(Info::new("Yydra Product Public API", "1.0.0"), Paths::new());
    let mut router =
        OpenApiRouter::with_openapi(openapi).routes(routes!(get_framework_contract_profile));
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
    for response_schema in ["FrameworkContractProfile", "ProblemDetails"] {
        let Some(RefOr::T(Schema::Object(schema))) = schemas.get_mut(response_schema) else {
            panic!("{response_schema} must be a collected object schema");
        };
        schema.additional_properties = Some(Box::new(AdditionalProperties::FreeForm(true)));
    }
    router
}

/// Deterministic, normalized Public API Contract derived from `public_routes`.
pub fn normalized_openapi_json() -> Result<String, serde_json::Error> {
    let (_, openapi) = public_routes().split_for_parts();
    let value = serde_json::to_value(openapi)?;
    let mut bytes = serde_json::to_vec_pretty(&value)?;
    bytes.push(b'\n');
    String::from_utf8(bytes).map_err(|error| {
        serde_json::Error::io(std::io::Error::new(std::io::ErrorKind::InvalidData, error))
    })
}

pub fn router(service: HealthService) -> Router {
    let public_router: Router = public_routes().into();
    Router::new()
        .route("/health", get(health))
        .merge(public_router.with_state::<HealthService>(()))
        .with_state(service)
}
