// SPDX-License-Identifier: MIT OR Apache-2.0

#![forbid(unsafe_code)]

use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::get;
use axum::{Json, Router};
use product_application::HealthService;
use serde::Serialize;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct HealthResponse {
    status: &'static str,
    database: String,
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

pub fn router(service: HealthService) -> Router {
    Router::new()
        .route("/health", get(health))
        .with_state(service)
}
