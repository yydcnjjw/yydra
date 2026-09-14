// SPDX-License-Identifier: MIT OR Apache-2.0
//! Controlled OAuth provider for explicit test builds. Never enabled by production assembly.
use crate::{hash, random_secret, valid_challenge, valid_verifier};
use axum::{
    Json, Router,
    extract::{Form, Query, State},
    http::{HeaderMap, StatusCode},
    response::{Html, IntoResponse, Redirect, Response},
    routing::{get, post},
};
use oauth2::url::Url;
use serde::Deserialize;
use serde_json::json;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};
use time::{Duration, OffsetDateTime};

#[derive(Clone, Default)]
struct Fixture(Arc<Mutex<HashMap<String, Grant>>>);
struct Grant {
    challenge: String,
    subject: u64,
    expires: OffsetDateTime,
}
#[derive(Deserialize)]
struct Authorization {
    redirect_uri: String,
    state: String,
    code_challenge: String,
    code_challenge_method: String,
    user: Option<String>,
}
fn callback(input: &Authorization) -> Result<Url, StatusCode> {
    let url = Url::parse(&input.redirect_uri).map_err(|_| StatusCode::BAD_REQUEST)?;
    if url.scheme() != "http"
        || !matches!(url.host_str(), Some("localhost" | "127.0.0.1"))
        || url.path() != "/auth/github/callback"
        || input.state.len() > 256
        || input.code_challenge_method != "S256"
        || !valid_challenge(&input.code_challenge)
    {
        return Err(StatusCode::BAD_REQUEST);
    }
    Ok(url)
}
async fn authorize(Query(input): Query<Authorization>) -> Result<Html<String>, StatusCode> {
    callback(&input)?;
    let mut base = Url::parse("http://localhost/approve").unwrap();
    base.query_pairs_mut().extend_pairs([
        ("redirect_uri", input.redirect_uri.as_str()),
        ("state", &input.state),
        ("code_challenge", &input.code_challenge),
        ("code_challenge_method", "S256"),
    ]);
    let mut links = String::new();
    for (user, label) in [("1", "Account A"), ("2", "Account B"), ("cancel", "Cancel")] {
        let mut link = base.clone();
        link.query_pairs_mut().append_pair("user", user);
        let href = format!("{}?{}", link.path(), link.query().unwrap())
            .replace('&', "&amp;")
            .replace('"', "&quot;");
        links.push_str(&format!("<p><a href=\"{href}\">{label}</a></p>"));
    }
    Ok(Html(format!(
        "<!doctype html><html><head><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\"></head><body><h1>Controlled sign-in fixture</h1>{links}</body></html>"
    )))
}
async fn approve(
    State(fixture): State<Fixture>,
    Query(input): Query<Authorization>,
) -> Result<Redirect, StatusCode> {
    let mut target = callback(&input)?;
    target.query_pairs_mut().append_pair("state", &input.state);
    if input.user.as_deref() == Some("cancel") {
        target
            .query_pairs_mut()
            .append_pair("error", "access_denied");
    } else {
        let subject = match input.user.as_deref() {
            Some("1") => 1,
            Some("2") => 2,
            _ => return Err(StatusCode::BAD_REQUEST),
        };
        let code = random_secret();
        let mut grants = fixture.0.lock().unwrap();
        grants.retain(|_, grant| grant.expires > OffsetDateTime::now_utc());
        if grants.len() >= 1000 {
            return Err(StatusCode::TOO_MANY_REQUESTS);
        }
        grants.insert(
            code.clone(),
            Grant {
                challenge: input.code_challenge,
                subject,
                expires: OffsetDateTime::now_utc() + Duration::minutes(1),
            },
        );
        target.query_pairs_mut().append_pair("code", &code);
    }
    Ok(Redirect::to(target.as_str()))
}
async fn token(
    State(fixture): State<Fixture>,
    Form(input): Form<HashMap<String, String>>,
) -> Response {
    if input.get("client_id").map(String::as_str) != Some("fixture-client")
        || input.get("client_secret").map(String::as_str) != Some("fixture-secret")
    {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let Some(code) = input.get("code") else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    let Some(verifier) = input.get("code_verifier") else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    let Some(grant) = fixture.0.lock().unwrap().remove(code) else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    if !valid_verifier(verifier)
        || hash(verifier) != grant.challenge
        || grant.expires <= OffsetDateTime::now_utc()
    {
        return StatusCode::BAD_REQUEST.into_response();
    }
    Json(json!({"access_token":format!("fixture-user-{}", grant.subject),"token_type":"bearer"}))
        .into_response()
}
async fn user(headers: HeaderMap) -> Response {
    match headers.get("authorization").and_then(|v| v.to_str().ok()) {
        Some("Bearer fixture-user-1") => Json(json!({"id":1,"login":"fixture-a"})).into_response(),
        Some("Bearer fixture-user-2") => Json(json!({"id":2,"login":"fixture-b"})).into_response(),
        _ => StatusCode::UNAUTHORIZED.into_response(),
    }
}
/// Mount outside AuthService's native credential adapter in an explicit test server.
pub fn router() -> Router {
    Router::new()
        .route("/authorize", get(authorize))
        .route("/approve", get(approve))
        .route("/token", post(token))
        .route("/user", get(user))
        .with_state(Fixture::default())
}
