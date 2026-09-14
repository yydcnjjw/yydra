// SPDX-License-Identifier: MIT OR Apache-2.0
use super::*;
use axum::{
    body::{Body, to_bytes},
    extract::Form,
};
use oauth2::url::Url;
use serde_json::{Value, json};
use tower::ServiceExt;

fn config() -> AuthConfig {
    AuthConfig::new(
        "http://127.0.0.1:4000",
        "http://127.0.0.1:8081",
        "test-product://auth/callback",
        true,
    )
    .unwrap()
}
#[test]
fn pkce_matches_rfc7636_and_rejects_malformed_input() {
    assert_eq!(
        hash("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"),
        "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
    );
    assert!(valid_challenge(
        "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
    ));
    assert!(!valid_challenge(&"a".repeat(42)));
    assert!(!valid_verifier(&" ".repeat(43)));
}
#[test]
fn configuration_rejects_insecure_production_and_unsafe_returns() {
    assert!(
        AuthConfig::new(
            "http://api.example.com",
            "https://app.example.com",
            "app://auth/callback",
            false
        )
        .is_err()
    );
    assert!(
        AuthConfig::new(
            "https://api.example.com",
            "https://app.example.com",
            "javascript:bad",
            false
        )
        .is_err()
    );
    assert!(config().lifetime_seconds(0).is_err());
    assert!(config().test_provider("https://attacker.example").is_err());
}
async fn request(
    app: &Router,
    method: Method,
    path: &str,
    cookie: Option<&str>,
    bearer: Option<&str>,
    body: Value,
    csrf: Option<&str>,
) -> Response {
    let mut builder = axum::http::Request::builder().method(method).uri(path);
    if let Some(cookie) = cookie {
        builder = builder.header(header::COOKIE, cookie);
    }
    if let Some(bearer) = bearer {
        builder = builder.header(header::AUTHORIZATION, format!("Bearer {bearer}"));
    }
    if let Some(csrf) = csrf {
        builder = builder
            .header(header::ORIGIN, "http://127.0.0.1:8081")
            .header("x-yydra-csrf", csrf);
    }
    app.clone()
        .oneshot(
            builder
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap()
}
async fn json_body(response: Response) -> Value {
    serde_json::from_slice(&to_bytes(response.into_body(), 100_000).await.unwrap()).unwrap()
}
fn cookie(response: &Response) -> String {
    response
        .headers()
        .get_all(header::SET_COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .find(|v| v.starts_with("yydra-session="))
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_owned()
}
async fn begin(app: &Router, native: Option<&str>) -> (String, String) {
    let path = native
        .map(|s| format!("/auth/github?native_challenge={s}"))
        .unwrap_or_else(|| "/auth/github".into());
    let response = request(app, Method::GET, &path, None, None, json!(null), None).await;
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    let cookie = cookie(&response);
    let url = Url::parse(
        response
            .headers()
            .get(header::LOCATION)
            .unwrap()
            .to_str()
            .unwrap(),
    )
    .unwrap();
    assert!(
        url.query_pairs()
            .any(|(k, v)| k == "code_challenge_method" && v == "S256")
    );
    let state = url
        .query_pairs()
        .find(|(k, _)| k == "state")
        .unwrap()
        .1
        .into_owned();
    (cookie, state)
}
async fn login(app: &Router, github_id: &str) -> String {
    let (binding, state) = begin(app, None).await;
    let response = request(
        app,
        Method::GET,
        &format!("/auth/github/callback?state={state}&code={github_id}"),
        Some(&binding),
        None,
        json!(null),
        None,
    )
    .await;
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    let session = cookie(&response);
    assert_ne!(
        session, binding,
        "login rotates the pre-authentication session"
    );
    let replay = request(
        app,
        Method::GET,
        &format!("/auth/github/callback?state={state}&code={github_id}"),
        Some(&binding),
        None,
        json!(null),
        None,
    )
    .await;
    assert_eq!(replay.status(), StatusCode::BAD_REQUEST);
    session
}
async fn session(app: &Router, cookie: Option<&str>, bearer: Option<&str>) -> Value {
    let response = request(
        app,
        Method::GET,
        "/api/v1/auth/session",
        cookie,
        bearer,
        json!(null),
        None,
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    json_body(response).await
}

#[tokio::test]
async fn unconfigured_signin_and_untrusted_credentials_fail_closed() {
    let pool = sqlx::postgres::PgPoolOptions::new()
        .connect_lazy("postgres://invalid/unused")
        .unwrap();
    let app = AuthService::new(pool, config())
        .unwrap()
        .layer(Router::new());
    let response = request(
        &app,
        Method::GET,
        "/auth/github",
        None,
        None,
        json!(null),
        None,
    )
    .await;
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    let response = request(
        &app,
        Method::GET,
        "/api/v1/auth/session",
        Some("unrelated=cookie"),
        Some("bad"),
        json!(null),
        None,
    )
    .await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    let response = request(
        &app,
        Method::POST,
        "/api/v1/auth/native/exchange",
        None,
        None,
        json!({"handoff":"bad","verifier":"bad"}),
        None,
    )
    .await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
#[ignore = "requires a disposable PostgreSQL database in YYDRA_AUTH_TEST_DATABASE_URL"]
async fn postgres_oauth_sessions_replay_expiry_and_native_handoff() {
    let url = std::env::var("YYDRA_AUTH_TEST_DATABASE_URL").expect("isolated test DB");
    let pool = PgPool::connect(&url).await.unwrap();
    sqlx::raw_sql(MIGRATION).execute(&pool).await.unwrap();
    async fn token(Form(form): Form<std::collections::HashMap<String, String>>) -> Json<Value> {
        assert!(form.get("code_verifier").is_some_and(|v| valid_verifier(v)));
        assert_eq!(form["client_id"], "test-client");
        assert_eq!(form["client_secret"], "test-secret");
        Json(json!({"access_token": form["code"], "token_type": "bearer"}))
    }
    async fn user(headers: axum::http::HeaderMap) -> Json<Value> {
        let id = headers[header::AUTHORIZATION]
            .to_str()
            .unwrap()
            .strip_prefix("Bearer ")
            .unwrap()
            .parse::<u64>()
            .unwrap();
        Json(json!({"id":id, "login":"mutable-handle"}))
    }
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let provider_url = format!("http://{}", listener.local_addr().unwrap());
    let provider = tokio::spawn(async move {
        axum::serve(
            listener,
            Router::new()
                .route("/token", post(token))
                .route("/user", get(user)),
        )
        .await
        .unwrap();
    });
    let service = AuthService::new(
        pool.clone(),
        config()
            .github("test-client".into(), "test-secret".into())
            .unwrap()
            .test_provider(&provider_url)
            .unwrap(),
    )
    .unwrap();
    let app = service.layer(Router::new().route(
        "/private",
        get(|principal: Option<Extension<Principal>>| async move {
            if principal.is_some() {
                StatusCode::OK
            } else {
                StatusCode::UNAUTHORIZED
            }
        }),
    ));

    let (cookie_a, cookie_b) = tokio::join!(login(&app, "1"), login(&app, "1"));
    let a = session(&app, Some(&cookie_a), None).await;
    let b = session(&app, Some(&cookie_b), None).await;
    assert_eq!(a["accountId"], b["accountId"]);
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM yydra_auth_accounts")
            .fetch_one(&pool)
            .await
            .unwrap(),
        1
    );
    let cookie_c = login(&app, "2").await;
    assert_ne!(
        a["accountId"],
        session(&app, Some(&cookie_c), None).await["accountId"]
    );
    let sid_a = cookie_a
        .split_once('=')
        .unwrap()
        .1
        .parse::<tower_sessions::session::Id>()
        .unwrap();
    use tower_sessions::SessionStore;
    let store = PostgresStore::new(pool.clone());
    let old_record = store.load(&sid_a).await.unwrap().unwrap();
    let denied = request(
        &app,
        Method::POST,
        "/api/v1/auth/logout",
        Some(&cookie_a),
        None,
        json!({}),
        None,
    )
    .await;
    assert_eq!(denied.status(), StatusCode::FORBIDDEN);
    let expired_at_before = sqlx::query_scalar::<_, OffsetDateTime>(
        "SELECT expires_at FROM yydra_auth_active_sessions WHERE id = $1",
    )
    .bind(sid_a.to_string())
    .fetch_one(&pool)
    .await
    .unwrap();
    session(&app, Some(&cookie_a), None).await;
    assert_eq!(
        store.load(&sid_a).await.unwrap().unwrap().expiry_date,
        expired_at_before
    );
    let signed_out = request(
        &app,
        Method::POST,
        "/api/v1/auth/logout",
        Some(&cookie_a),
        None,
        json!({}),
        a["csrfToken"].as_str(),
    )
    .await;
    assert_eq!(signed_out.status(), StatusCode::OK);
    store.save(&old_record).await.unwrap(); // Simulate a late request's upstream UPSERT.
    assert!(session(&app, Some(&cookie_a), None).await["accountId"].is_null());
    assert_eq!(
        session(&app, Some(&cookie_b), None).await["accountId"],
        b["accountId"]
    );
    sqlx::query("UPDATE yydra_auth_active_sessions SET expires_at = CURRENT_TIMESTAMP - INTERVAL '1 second' WHERE id = $1").bind(cookie_b.split_once('=').unwrap().1).execute(&pool).await.unwrap();
    assert!(session(&app, Some(&cookie_b), None).await["accountId"].is_null());

    let verifier = "A".repeat(43);
    for native in [false, true] {
        let challenge = hash(&verifier);
        let (binding, state) = begin(&app, native.then_some(challenge.as_str())).await;
        let denied = request(
            &app,
            Method::GET,
            &format!("/auth/github/callback?state={state}&error=access_denied"),
            Some(&binding),
            None,
            json!(null),
            None,
        )
        .await;
        assert_eq!(denied.status(), StatusCode::SEE_OTHER);
        let target = Url::parse(denied.headers()[header::LOCATION].to_str().unwrap()).unwrap();
        assert_eq!(
            target.scheme(),
            if native { "test-product" } else { "http" }
        );
        assert!(
            target
                .query_pairs()
                .any(|(key, value)| key == "authError" && value == "sign-in-not-completed")
        );
        assert!(session(&app, Some(&binding), None).await["accountId"].is_null());
    }
    let (binding, state) = begin(&app, Some(&hash(&verifier))).await;
    let wrong_binding = request(
        &app,
        Method::GET,
        &format!("/auth/github/callback?state={state}&code=1"),
        Some(&cookie_c),
        None,
        json!(null),
        None,
    )
    .await;
    assert_eq!(wrong_binding.status(), StatusCode::BAD_REQUEST);
    let callback = request(
        &app,
        Method::GET,
        &format!("/auth/github/callback?state={state}&code=1"),
        Some(&binding),
        None,
        json!(null),
        None,
    )
    .await;
    assert_eq!(callback.status(), StatusCode::SEE_OTHER);
    let target = Url::parse(callback.headers()[header::LOCATION].to_str().unwrap()).unwrap();
    assert_eq!(target.scheme(), "test-product");
    let handoff = target
        .query_pairs()
        .find(|(k, _)| k == "handoff")
        .unwrap()
        .1
        .into_owned();
    sqlx::query("INSERT INTO yydra_auth_handoffs (code_hash, challenge, account_id, expires_at) SELECT $1, challenge, account_id, CURRENT_TIMESTAMP - INTERVAL '1 second' FROM yydra_auth_handoffs WHERE code_hash = $2")
        .bind(hash("expired-handoff")).bind(hash(&handoff)).execute(&pool).await.unwrap();
    let expired = request(
        &app,
        Method::POST,
        "/api/v1/auth/native/exchange",
        None,
        None,
        json!({"handoff":"expired-handoff", "verifier":verifier}),
        None,
    )
    .await;
    assert_eq!(expired.status(), StatusCode::BAD_REQUEST);
    let wrong = request(
        &app,
        Method::POST,
        "/api/v1/auth/native/exchange",
        None,
        None,
        json!({"handoff":handoff,"verifier":"B".repeat(43)}),
        None,
    )
    .await;
    assert_eq!(wrong.status(), StatusCode::BAD_REQUEST);
    let exchange = request(
        &app,
        Method::POST,
        "/api/v1/auth/native/exchange",
        None,
        None,
        json!({"handoff":handoff,"verifier":verifier}),
        None,
    )
    .await;
    assert_eq!(exchange.status(), StatusCode::OK);
    assert!(!exchange.headers().contains_key(header::SET_COOKIE));
    let native = json_body(exchange).await;
    let credential = native["credential"].as_str().unwrap();
    assert_eq!(
        session(&app, None, Some(credential)).await["accountId"],
        a["accountId"]
    );
    let replay = request(
        &app,
        Method::POST,
        "/api/v1/auth/native/exchange",
        None,
        None,
        json!({"handoff":handoff,"verifier":verifier}),
        None,
    )
    .await;
    assert_eq!(replay.status(), StatusCode::BAD_REQUEST);
    let wrong_transport = request(
        &app,
        Method::GET,
        "/private",
        Some(&format!("yydra-session={credential}")),
        None,
        json!(null),
        None,
    )
    .await;
    assert_eq!(wrong_transport.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(
        request(
            &app,
            Method::POST,
            "/api/v1/auth/logout",
            None,
            Some(credential),
            json!({}),
            None
        )
        .await
        .status(),
        StatusCode::OK
    );
    assert!(session(&app, None, Some(credential)).await["accountId"].is_null());
    service.cleanup().await.unwrap();
    provider.abort();
}
