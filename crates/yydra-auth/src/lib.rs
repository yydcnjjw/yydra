// SPDX-License-Identifier: MIT OR Apache-2.0
//! GitHub identity verification and independently revocable product sessions.
#![forbid(unsafe_code)]

mod config;
#[cfg(feature = "test-provider")]
pub mod test_provider;
pub use config::AuthConfig;

use axum::{
    Extension, Json, Router,
    extract::{Query, Request, State},
    http::{HeaderValue, Method, StatusCode, header},
    middleware::{self, Next},
    response::{IntoResponse, Redirect, Response},
    routing::{get, post},
};
use axum_login::{AuthManagerLayerBuilder, AuthSession, AuthUser, AuthnBackend};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use oauth2::{
    AuthUrl, AuthorizationCode, ClientId, ClientSecret, CsrfToken, PkceCodeChallenge,
    PkceCodeVerifier, RedirectUrl, TokenResponse, TokenUrl, basic::BasicClient, reqwest,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use std::{fmt, sync::Arc};
use time::{Duration, OffsetDateTime, format_description::well_known::Rfc3339};
use tower_http::cors::CorsLayer;
use tower_sessions::{Expiry, SessionManagerLayer, cookie::SameSite};
use tower_sessions_sqlx_store::PostgresStore;
use utoipa::ToSchema;

pub const MIGRATION: &str = include_str!("../migrations/0001_auth.sql");

/// Validated request identity. Product code must still enforce resource ownership.
#[derive(Clone, Debug)]
pub struct Principal {
    pub account_id: String,
}

#[derive(Clone)]
pub struct AuthService {
    pool: PgPool,
    config: Arc<AuthConfig>,
    http: reqwest::Client,
}

#[derive(Clone, sqlx::FromRow)]
struct Account {
    id: String,
    auth_epoch: String,
}
impl fmt::Debug for Account {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Account")
            .field("id", &self.id)
            .finish_non_exhaustive()
    }
}
impl AuthUser for Account {
    type Id = String;
    fn id(&self) -> String {
        self.id.clone()
    }
    fn session_auth_hash(&self) -> &[u8] {
        self.auth_epoch.as_bytes()
    }
}
#[derive(Clone)]
struct Backend(AuthService);
struct Credentials {
    code: String,
    verifier: String,
}
impl AuthnBackend for Backend {
    type User = Account;
    type Credentials = Credentials;
    type Error = AuthError;
    async fn authenticate(&self, credentials: Credentials) -> Result<Option<Account>, AuthError> {
        let cfg = &self.0.config;
        let token = BasicClient::new(ClientId::new(
            cfg.client_id.clone().ok_or_else(AuthError::configuration)?,
        ))
        .set_auth_type(oauth2::AuthType::RequestBody)
        .set_client_secret(ClientSecret::new(
            cfg.client_secret
                .clone()
                .ok_or_else(AuthError::configuration)?,
        ))
        .set_token_uri(TokenUrl::new(cfg.token_url.clone()).expect("fixed URL"))
        .set_redirect_uri(RedirectUrl::new(cfg.callback()).expect("validated callback"))
        .exchange_code(AuthorizationCode::new(credentials.code))
        .set_pkce_verifier(PkceCodeVerifier::new(credentials.verifier))
        .request_async(&self.0.http)
        .await
        .map_err(|_| AuthError::provider())?;
        #[derive(Deserialize)]
        struct GitHubIdentity {
            id: u64,
        }
        let identity = self
            .0
            .http
            .get(&cfg.user_url)
            .bearer_auth(token.access_token().secret())
            .header("Accept", "application/vnd.github+json")
            .header("X-GitHub-Api-Version", "2026-03-10")
            .send()
            .await
            .map_err(|_| AuthError::provider())?
            .error_for_status()
            .map_err(|_| AuthError::provider())?
            .json::<GitHubIdentity>()
            .await
            .map_err(|_| AuthError::provider())?;
        if identity.id == 0 {
            return Err(AuthError::provider());
        }
        // The unique provider/subject constraint makes concurrent provisioning atomic.
        let account = sqlx::query_as::<_, Account>(
            "INSERT INTO yydra_auth_accounts (provider, subject, auth_epoch) VALUES ('github', $1, $2)
             ON CONFLICT (provider, subject) DO UPDATE SET subject = EXCLUDED.subject
             RETURNING id::text AS id, auth_epoch")
            .bind(identity.id.to_string()).bind(random_secret()).fetch_one(&self.0.pool).await?;
        Ok(Some(account))
    }
    async fn get_user(&self, id: &String) -> Result<Option<Account>, AuthError> {
        self.0.account(id).await
    }
}
type LoginSession = AuthSession<Backend>;

#[derive(Clone, Debug, sqlx::FromRow)]
struct Lease {
    account_id: String,
    expires_at: OffsetDateTime,
    transport: String,
    csrf_token: String,
}
#[derive(Clone)]
struct NativeRequest;

impl AuthService {
    /// No-database service for exercising anonymous JSON contracts in test builds.
    #[cfg(feature = "test-provider")]
    pub fn contract_fixture() -> Self {
        let pool = sqlx::postgres::PgPoolOptions::new()
            .connect_lazy("postgres://invalid/contract-fixture")
            .expect("fixed URL");
        Self::new(
            pool,
            AuthConfig::new(
                "http://127.0.0.1:4000",
                "http://127.0.0.1:8081",
                "fixture://auth/callback",
                true,
            )
            .expect("fixture config"),
        )
        .expect("fixture service")
    }

    pub fn new(pool: PgPool, config: AuthConfig) -> Result<Self, AuthError> {
        let http = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(std::time::Duration::from_secs(15))
            .user_agent("yydra-auth")
            .build()
            .map_err(|_| AuthError::configuration())?;
        Ok(Self {
            pool,
            config: Arc::new(config),
            http,
        })
    }

    /// Install auth routes and request identity/CSRF checks around a product router.
    pub fn layer(&self, product: Router) -> Router {
        let routes = Router::new()
            .route("/auth/github", get(start))
            .route("/auth/github/callback", get(callback))
            .route("/api/v1/auth/session", get(current_session))
            .route("/api/v1/auth/logout", post(logout))
            .route("/api/v1/auth/native/exchange", post(native_exchange))
            .with_state(self.clone());
        let sessions = SessionManagerLayer::new(PostgresStore::new(self.pool.clone()))
            .with_name(self.config.cookie_name())
            .with_secure(self.config.secure)
            .with_http_only(true)
            .with_same_site(SameSite::Lax)
            .with_expiry(Expiry::OnInactivity(Duration::minutes(10)));
        let auth = AuthManagerLayerBuilder::new(Backend(self.clone()), sessions).build();
        let origin: HeaderValue = self
            .config
            .web_return
            .origin()
            .ascii_serialization()
            .parse()
            .expect("validated origin");
        let cors = CorsLayer::new()
            .allow_origin(origin)
            .allow_credentials(true)
            .expose_headers([header::WWW_AUTHENTICATE])
            .allow_methods([Method::GET, Method::POST, Method::PATCH, Method::OPTIONS])
            .allow_headers([
                header::CONTENT_TYPE,
                header::AUTHORIZATION,
                header::HeaderName::from_static("x-yydra-csrf"),
            ]);
        product
            .merge(routes)
            .layer(middleware::from_fn_with_state(self.clone(), identity))
            .layer(auth)
            .layer(middleware::from_fn_with_state(self.clone(), transport))
            .layer(cors)
    }

    async fn account(&self, id: &str) -> Result<Option<Account>, AuthError> {
        Ok(sqlx::query_as::<_, Account>(
            "SELECT id::text AS id, auth_epoch FROM yydra_auth_accounts WHERE id = $1::uuid",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?)
    }

    async fn lease(&self, session: &LoginSession) -> Result<Option<Lease>, AuthError> {
        let (Some(id), Some(user)) = (session.session.id(), &session.user) else {
            return Ok(None);
        };
        Ok(sqlx::query_as::<_, Lease>(
            "SELECT account_id::text AS account_id, expires_at, transport, csrf_token
             FROM yydra_auth_active_sessions WHERE id = $1 AND account_id = $2::uuid AND expires_at > CURRENT_TIMESTAMP")
            .bind(id.to_string()).bind(&user.id).fetch_optional(&self.pool).await?)
    }

    async fn revoke(&self, session: &mut LoginSession) -> Result<(), AuthError> {
        if let Some(id) = session.session.id() {
            sqlx::query("DELETE FROM yydra_auth_active_sessions WHERE id = $1")
                .bind(id.to_string())
                .execute(&self.pool)
                .await?;
        }
        session
            .logout()
            .await
            .map_err(|_| AuthError::unavailable())?;
        Ok(())
    }

    async fn establish(
        &self,
        session: &mut LoginSession,
        account: &Account,
        transport: &str,
    ) -> Result<SessionView, AuthError> {
        self.revoke(session).await?;
        session
            .login(account)
            .await
            .map_err(|_| AuthError::unavailable())?;
        let expires_at =
            OffsetDateTime::now_utc() + Duration::seconds(self.config.lifetime_seconds);
        session
            .session
            .set_expiry(Some(Expiry::AtDateTime(expires_at)));
        session
            .session
            .save()
            .await
            .map_err(|_| AuthError::unavailable())?;
        let id = session
            .session
            .id()
            .ok_or_else(AuthError::unavailable)?
            .to_string();
        let csrf_token = random_secret();
        sqlx::query("INSERT INTO yydra_auth_active_sessions (id, account_id, expires_at, transport, csrf_token) VALUES ($1, $2::uuid, $3, $4, $5)")
            .bind(id).bind(&account.id).bind(expires_at).bind(transport).bind(&csrf_token)
            .execute(&self.pool).await?;
        Ok(SessionView::authenticated(
            Lease {
                account_id: account.id.clone(),
                expires_at,
                transport: transport.into(),
                csrf_token,
            },
            self.config.available(),
        ))
    }

    /// The host calls this periodically; startup never performs migrations.
    pub async fn cleanup(&self) -> Result<(), AuthError> {
        for query in [
            "DELETE FROM yydra_auth_attempts WHERE expires_at <= CURRENT_TIMESTAMP",
            "DELETE FROM yydra_auth_handoffs WHERE expires_at <= CURRENT_TIMESTAMP",
            "DELETE FROM yydra_auth_active_sessions WHERE expires_at <= CURRENT_TIMESTAMP",
            "DELETE FROM tower_sessions.session WHERE expiry_date <= CURRENT_TIMESTAMP",
        ] {
            sqlx::query(query).execute(&self.pool).await?;
        }
        Ok(())
    }
}

// Native clients explicitly mark handoff redemption, since they have no token yet.
async fn transport(
    State(service): State<AuthService>,
    mut request: Request,
    next: Next,
) -> Response {
    let native = request.headers().contains_key(header::AUTHORIZATION)
        || request.uri().path() == "/api/v1/auth/native/exchange";
    if native {
        if request.headers().contains_key(header::COOKIE) {
            return AuthError::unauthorized().into_response();
        }
        if let Some(auth) = request.headers_mut().remove(header::AUTHORIZATION) {
            let Some(id) = auth
                .to_str()
                .ok()
                .and_then(|s| s.strip_prefix("Bearer "))
                .and_then(|s| s.parse::<tower_sessions::session::Id>().ok())
            else {
                return AuthError::unauthorized().into_response();
            };
            let cookie = format!("{}={id}", service.config.cookie_name());
            request
                .headers_mut()
                .insert(header::COOKIE, cookie.parse().expect("encoded session ID"));
        }
        request.extensions_mut().insert(NativeRequest);
    }
    let mut response = next.run(request).await;
    if response.status() == StatusCode::INTERNAL_SERVER_ERROR
        && response
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            != Some("application/problem+json")
    {
        response = AuthError::unavailable().into_response();
    }
    if native {
        response.headers_mut().remove(header::SET_COOKIE);
    }
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response.headers_mut().insert(
        header::REFERRER_POLICY,
        HeaderValue::from_static("no-referrer"),
    );
    response
}

async fn identity(
    State(service): State<AuthService>,
    session: LoginSession,
    mut request: Request,
    next: Next,
) -> Result<Response, AuthError> {
    let native = request.extensions().get::<NativeRequest>().is_some();
    if let Some(lease) = service.lease(&session).await? {
        if (lease.transport == "native") != native {
            return Err(AuthError::unauthorized());
        }
        if !native
            && !matches!(
                *request.method(),
                Method::GET | Method::HEAD | Method::OPTIONS
            )
        {
            let origin = request
                .headers()
                .get(header::ORIGIN)
                .and_then(|v| v.to_str().ok());
            let csrf = request
                .headers()
                .get("x-yydra-csrf")
                .and_then(|v| v.to_str().ok());
            if origin
                != Some(
                    service
                        .config
                        .web_return
                        .origin()
                        .ascii_serialization()
                        .as_str(),
                )
                || csrf != Some(lease.csrf_token.as_str())
            {
                return Err(AuthError::forbidden());
            }
        }
        // Loading a stored Record does not restore Session's configured expiry.
        session
            .session
            .set_expiry(Some(Expiry::AtDateTime(lease.expires_at)));
        request.extensions_mut().insert(Principal {
            account_id: lease.account_id.clone(),
        });
        request.extensions_mut().insert(lease);
    }
    Ok(next.run(request).await)
}

#[derive(Deserialize)]
struct StartQuery {
    native_challenge: Option<String>,
}
async fn start(
    State(service): State<AuthService>,
    session: LoginSession,
    Query(query): Query<StartQuery>,
) -> Result<Redirect, AuthError> {
    let config = &service.config;
    if !config.available() {
        return Err(AuthError::configuration());
    }
    if query
        .native_challenge
        .as_ref()
        .is_some_and(|s| !valid_challenge(s))
    {
        return Err(AuthError::invalid());
    }
    let (challenge, verifier) = PkceCodeChallenge::new_random_sha256();
    let binding = random_secret();
    session
        .session
        .insert("github_binding", &binding)
        .await
        .map_err(|_| AuthError::unavailable())?;
    let client = BasicClient::new(ClientId::new(config.client_id.clone().expect("available")))
        .set_auth_uri(AuthUrl::new(config.authorize_url.clone()).expect("fixed URL"))
        .set_redirect_uri(RedirectUrl::new(config.callback()).expect("validated callback"));
    let (url, state) = client
        .authorize_url(CsrfToken::new_random)
        .set_pkce_challenge(challenge)
        .add_extra_param("prompt", "select_account")
        .url();
    sqlx::query("INSERT INTO yydra_auth_attempts (state_hash, binding_hash, verifier, native_challenge, expires_at) VALUES ($1, $2, $3, $4, CURRENT_TIMESTAMP + INTERVAL '10 minutes')")
        .bind(hash(state.secret())).bind(hash(&binding)).bind(verifier.secret()).bind(query.native_challenge)
        .execute(&service.pool).await?;
    Ok(Redirect::to(url.as_str()))
}

#[derive(Deserialize)]
struct CallbackQuery {
    code: Option<String>,
    state: Option<String>,
    error: Option<String>,
}
#[derive(sqlx::FromRow)]
struct Attempt {
    verifier: String,
    native_challenge: Option<String>,
}
async fn callback(
    State(service): State<AuthService>,
    mut session: LoginSession,
    Query(query): Query<CallbackQuery>,
) -> Result<Redirect, AuthError> {
    let state = query
        .state
        .filter(|s| s.len() <= 256)
        .ok_or_else(AuthError::invalid)?;
    let binding = session
        .session
        .get::<String>("github_binding")
        .await
        .map_err(|_| AuthError::unavailable())?
        .ok_or_else(AuthError::invalid)?;
    let attempt = sqlx::query_as::<_, Attempt>("DELETE FROM yydra_auth_attempts WHERE state_hash = $1 AND binding_hash = $2 AND expires_at > CURRENT_TIMESTAMP RETURNING verifier, native_challenge")
        .bind(hash(&state)).bind(hash(&binding)).fetch_optional(&service.pool).await?.ok_or_else(AuthError::invalid)?;
    session
        .session
        .remove_value("github_binding")
        .await
        .map_err(|_| AuthError::unavailable())?;
    if query.error.is_some() {
        return Ok(login_failure(
            &service.config,
            attempt.native_challenge.is_some(),
        ));
    }
    let code = query
        .code
        .filter(|s| !s.is_empty() && s.len() <= 1024)
        .ok_or_else(AuthError::invalid)?;
    let account = match session
        .authenticate(Credentials {
            code,
            verifier: attempt.verifier,
        })
        .await
    {
        Ok(Some(account)) => account,
        _ => {
            return Ok(login_failure(
                &service.config,
                attempt.native_challenge.is_some(),
            ));
        }
    };
    if let Some(challenge) = attempt.native_challenge {
        let handoff = random_secret();
        sqlx::query("INSERT INTO yydra_auth_handoffs (code_hash, account_id, challenge, expires_at) VALUES ($1, $2::uuid, $3, CURRENT_TIMESTAMP + INTERVAL '60 seconds')")
            .bind(hash(&handoff)).bind(&account.id).bind(challenge).execute(&service.pool).await?;
        let mut target = service.config.native_return.clone();
        target.query_pairs_mut().append_pair("handoff", &handoff);
        Ok(Redirect::to(target.as_str()))
    } else {
        service.establish(&mut session, &account, "web").await?;
        Ok(Redirect::to(service.config.web_return.as_str()))
    }
}

fn login_failure(config: &AuthConfig, native: bool) -> Redirect {
    let mut target = if native {
        config.native_return.clone()
    } else {
        config.web_return.clone()
    };
    target
        .query_pairs_mut()
        .append_pair("authError", "sign-in-not-completed");
    Redirect::to(target.as_str())
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SessionView {
    #[schema(required, nullable = true)]
    pub account_id: Option<String>,
    #[schema(required, nullable = true, format = DateTime)]
    pub expires_at: Option<String>,
    #[schema(required, nullable = true)]
    pub csrf_token: Option<String>,
    pub login_available: bool,
}
impl SessionView {
    fn authenticated(lease: Lease, available: bool) -> Self {
        Self {
            account_id: Some(lease.account_id),
            expires_at: Some(lease.expires_at.format(&Rfc3339).expect("valid timestamp")),
            csrf_token: (lease.transport == "web").then_some(lease.csrf_token),
            login_available: available,
        }
    }
}

#[utoipa::path(get, path = "/api/v1/auth/session", operation_id = "getProductSession", tag = "auth",
    responses((status = 200, description = "Current product session", body = SessionView, content_type = "application/json"),
    (status = 401, description = "Invalid credentials or transport", body = AuthProblem, content_type = "application/problem+json"),
    (status = 503, description = "Session service unavailable", body = AuthProblem, content_type = "application/problem+json")))]
async fn current_session(
    State(service): State<AuthService>,
    lease: Option<Extension<Lease>>,
) -> Json<SessionView> {
    Json(match lease {
        Some(Extension(lease)) => SessionView::authenticated(lease, service.config.available()),
        None => SessionView {
            account_id: None,
            expires_at: None,
            csrf_token: None,
            login_available: service.config.available(),
        },
    })
}
#[derive(Serialize, ToSchema)]
pub struct LogoutView {
    pub revoked: bool,
}

#[utoipa::path(post, path = "/api/v1/auth/logout", operation_id = "logoutProductSession", tag = "auth",
    responses((status = 200, description = "Current session revoked", body = LogoutView, content_type = "application/json"),
    (status = 401, description = "Invalid credentials or transport", body = AuthProblem, content_type = "application/problem+json"),
    (status = 403, description = "CSRF verification failed", body = AuthProblem, content_type = "application/problem+json"),
    (status = 503, description = "Session service unavailable", body = AuthProblem, content_type = "application/problem+json")))]
async fn logout(
    State(service): State<AuthService>,
    mut session: LoginSession,
) -> Result<Json<LogoutView>, AuthError> {
    service.revoke(&mut session).await?;
    Ok(Json(LogoutView { revoked: true }))
}

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct HandoffRequest {
    pub handoff: String,
    pub verifier: String,
}
#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct NativeSessionView {
    pub credential: String,
    pub session: SessionView,
}

#[utoipa::path(post, path = "/api/v1/auth/native/exchange", operation_id = "exchangeNativeHandoff", tag = "auth",
    request_body(content = HandoffRequest, content_type = "application/json"),
    responses((status = 200, description = "Native product session established", body = NativeSessionView, content_type = "application/json"),
    (status = 400, description = "Invalid or expired handoff", body = AuthProblem, content_type = "application/problem+json"),
    (status = 401, description = "Ambiguous credentials", body = AuthProblem, content_type = "application/problem+json"),
    (status = 503, description = "Session service unavailable", body = AuthProblem, content_type = "application/problem+json")))]
async fn native_exchange(
    State(service): State<AuthService>,
    mut session: LoginSession,
    payload: Result<Json<HandoffRequest>, axum::extract::rejection::JsonRejection>,
) -> Result<Json<NativeSessionView>, AuthError> {
    let Json(input) = payload.map_err(|_| AuthError::invalid())?;
    if !valid_verifier(&input.verifier) || input.handoff.len() > 128 {
        return Err(AuthError::invalid());
    }
    let id = sqlx::query_scalar::<_, String>("DELETE FROM yydra_auth_handoffs WHERE code_hash = $1 AND challenge = $2 AND expires_at > CURRENT_TIMESTAMP RETURNING account_id::text")
        .bind(hash(&input.handoff)).bind(hash(&input.verifier)).fetch_optional(&service.pool).await?.ok_or_else(AuthError::invalid)?;
    let account = service.account(&id).await?.ok_or_else(AuthError::invalid)?;
    let view = service.establish(&mut session, &account, "native").await?;
    Ok(Json(NativeSessionView {
        credential: session
            .session
            .id()
            .ok_or_else(AuthError::unavailable)?
            .to_string(),
        session: view,
    }))
}

#[derive(utoipa::OpenApi)]
#[openapi(paths(current_session, logout, native_exchange))]
struct AuthApi;
pub fn openapi() -> utoipa::openapi::OpenApi {
    <AuthApi as utoipa::OpenApi>::openapi()
}

#[derive(Debug, Clone)]
pub struct AuthError {
    status: StatusCode,
    code: &'static str,
}
impl AuthError {
    fn invalid() -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            code: "invalid-auth-request",
        }
    }
    fn forbidden() -> Self {
        Self {
            status: StatusCode::FORBIDDEN,
            code: "csrf-verification-failed",
        }
    }
    fn unauthorized() -> Self {
        Self {
            status: StatusCode::UNAUTHORIZED,
            code: "authentication-required",
        }
    }
    fn unavailable() -> Self {
        Self {
            status: StatusCode::SERVICE_UNAVAILABLE,
            code: "authentication-unavailable",
        }
    }
    fn configuration() -> Self {
        Self {
            status: StatusCode::SERVICE_UNAVAILABLE,
            code: "authentication-not-configured",
        }
    }
    fn provider() -> Self {
        Self {
            status: StatusCode::BAD_GATEWAY,
            code: "identity-provider-unavailable",
        }
    }
}
impl fmt::Display for AuthError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.code)
    }
}
impl std::error::Error for AuthError {}
impl From<sqlx::Error> for AuthError {
    fn from(_: sqlx::Error) -> Self {
        Self::unavailable()
    }
}
#[derive(Serialize, ToSchema)]
pub struct AuthProblem {
    #[serde(rename = "type")]
    pub type_uri: String,
    pub title: String,
    pub status: u16,
}
impl IntoResponse for AuthError {
    fn into_response(self) -> Response {
        let mut response = (
            self.status,
            [
                (header::CONTENT_TYPE, "application/problem+json"),
                (header::CACHE_CONTROL, "no-store"),
                (header::REFERRER_POLICY, "no-referrer"),
            ],
            Json(AuthProblem {
                type_uri: format!("https://yydra.dev/problems/{}", self.code),
                title: self.code.into(),
                status: self.status.as_u16(),
            }),
        )
            .into_response();
        if self.status == StatusCode::UNAUTHORIZED {
            response.headers_mut().insert(
                header::WWW_AUTHENTICATE,
                HeaderValue::from_static("Bearer realm=\"yydra-product\""),
            );
        }
        response
    }
}
fn random_secret() -> String {
    CsrfToken::new_random().secret().to_owned()
}
fn hash(input: &str) -> String {
    URL_SAFE_NO_PAD.encode(Sha256::digest(input.as_bytes()))
}
fn valid_challenge(s: &str) -> bool {
    s.len() == 43
        && s.bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
}
fn valid_verifier(s: &str) -> bool {
    (43..=128).contains(&s.len())
        && s.bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"-._~".contains(&c))
}

#[cfg(test)]
mod tests;
