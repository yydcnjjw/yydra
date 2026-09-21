// SPDX-License-Identifier: MIT OR Apache-2.0

use std::env;
use std::net::SocketAddr;

use product_application::{HealthService, ReadingQueueService};
use product_persistence_postgres::Database;
use snafu::{ResultExt, Snafu};
use tracing::info;
use tracing_subscriber::prelude::*;
use yydra_auth::{AuthConfig, AuthService};

#[tokio::main]
async fn main() -> std::process::ExitCode {
    // Session middleware formats raw backend errors before returning an opaque
    // status. The outer HTTP adapter owns the safe, correlated failure report.
    let filter = tracing_subscriber::EnvFilter::from_default_env();
    tracing_subscriber::registry()
        .with(filter)
        .with(
            tracing_subscriber::fmt::layer()
                .with_filter(tracing_subscriber::filter::filter_fn(safe_log_target)),
        )
        .init();
    match run().await {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            tracing::error!(code = error.code(), error = %error, "server stopped");
            std::process::ExitCode::FAILURE
        }
    }
}

fn safe_log_target(metadata: &tracing::Metadata<'_>) -> bool {
    !matches!(
        metadata.target().split("::").next(),
        Some("axum_login" | "tower_sessions" | "tower_sessions_core")
    )
}

async fn run() -> Result<(), ServerError> {
    let database_url = env::var("DATABASE_URL").context(EnvironmentSnafu {
        variable: "DATABASE_URL",
    })?;
    let database = Database::connect(&database_url, 4).await?;
    database.verify_compiled_migrations().await?;
    let development = env::var("YYDRA_AUTH_DEVELOPMENT").as_deref() == Ok("true");
    let mut config = AuthConfig::new(
        &env::var("YYDRA_PUBLIC_API_URL").unwrap_or_else(|_| {
            if development {
                "http://127.0.0.1:4000"
            } else {
                "https://localhost"
            }
            .into()
        }),
        &env::var("YYDRA_AUTH_WEB_RETURN").unwrap_or_else(|_| {
            if development {
                "http://127.0.0.1:8081"
            } else {
                "https://localhost"
            }
            .into()
        }),
        &env::var("YYDRA_AUTH_NATIVE_RETURN")
            .unwrap_or_else(|_| "__PRODUCT_ID__://auth/callback".into()),
        development,
    )?;
    match (
        env::var("GITHUB_CLIENT_ID").ok().filter(|v| !v.is_empty()),
        env::var("GITHUB_CLIENT_SECRET")
            .ok()
            .filter(|v| !v.is_empty()),
    ) {
        (Some(id), Some(secret)) => config = config.github(id, secret)?,
        (None, None) => {}
        _ => {
            return Err(ServerError::IncompleteGithubConfiguration);
        }
    }
    if let Ok(seconds) = env::var("YYDRA_AUTH_SESSION_SECONDS") {
        config = config.lifetime_seconds(seconds.parse().context(SessionLifetimeSnafu)?)?;
    }
    #[cfg(feature = "auth-fixture")]
    if let Ok(provider) = env::var("YYDRA_AUTH_FIXTURE_PROVIDER") {
        config = config.test_provider(&provider)?;
    }
    let authentication = AuthService::new(database.pool(), config)?;
    let cleanup = authentication.clone();
    let cleanup_task = tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(300));
        loop {
            interval.tick().await;
            if let Err(error) = cleanup.cleanup().await {
                tracing::warn!(
                    code = error.diagnostic_code(),
                    operation = error.operation(),
                    "authentication expiry cleanup failed"
                );
            }
        }
    });
    let cursor_signing_key =
        env::var("YYDRA_READING_QUEUE_CURSOR_SIGNING_KEY").context(EnvironmentSnafu {
            variable: "YYDRA_READING_QUEUE_CURSOR_SIGNING_KEY",
        })?;

    let app = authentication.layer(product_transport_http::router(
        HealthService::new(database.clone()),
        ReadingQueueService::new(database, cursor_signing_key.as_bytes())?,
    ));
    #[cfg(feature = "auth-fixture")]
    let app = app.merge(yydra_auth::test_provider::router());
    let address: SocketAddr = env::var("YYDRA_BIND_ADDRESS")
        .unwrap_or_else(|_| "127.0.0.1:4000".to_owned())
        .parse()
        .context(BindAddressSnafu)?;
    let listener = tokio::net::TcpListener::bind(address)
        .await
        .context(ListenSnafu)?;
    info!(%address, "serving Product Workspace");
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .context(ServeSnafu)?;
    cleanup_task.abort();
    Ok(())
}

async fn shutdown_signal() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{SignalKind, signal};

        let terminate = signal(SignalKind::terminate());
        match terminate {
            Ok(mut terminate) => {
                tokio::select! {
                    _ = tokio::signal::ctrl_c() => {}
                    _ = terminate.recv() => {}
                }
            }
            Err(_) => {
                let _ = tokio::signal::ctrl_c().await;
            }
        }
    }
    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
    }
}

#[derive(Debug, Snafu)]
enum ServerError {
    #[snafu(display("required environment variable {variable} is unavailable"))]
    Environment {
        variable: &'static str,
        source: env::VarError,
    },
    #[snafu(display("GITHUB_CLIENT_ID and GITHUB_CLIENT_SECRET must be configured together"))]
    IncompleteGithubConfiguration,
    #[snafu(context(false), display("authentication configuration failed"))]
    Authentication { source: yydra_auth::AuthError },
    #[snafu(context(false), display("connecting to the database failed"))]
    Database { source: sqlx::Error },
    #[snafu(context(false), display("database migration history is incompatible"))]
    Migration {
        source: product_persistence_postgres::MigrationError,
    },
    #[snafu(
        context(false),
        display("reading queue cursor configuration is invalid")
    )]
    Cursor {
        source: product_application::CursorConfigurationError,
    },
    #[snafu(display("session lifetime must be an integer"))]
    SessionLifetime { source: std::num::ParseIntError },
    #[snafu(display("server bind address is invalid"))]
    BindAddress { source: std::net::AddrParseError },
    #[snafu(display("could not bind server listener"))]
    Listen { source: std::io::Error },
    #[snafu(display("HTTP server failed"))]
    Serve { source: std::io::Error },
}
impl ServerError {
    fn code(&self) -> &'static str {
        match self {
            Self::Environment { .. } => "SERVER_ENVIRONMENT_MISSING",
            Self::IncompleteGithubConfiguration | Self::Authentication { .. } => {
                "SERVER_AUTH_CONFIG_INVALID"
            }
            Self::Database { .. } => "SERVER_DATABASE_UNAVAILABLE",
            Self::Migration { .. } => "SERVER_MIGRATION_INCOMPATIBLE",
            Self::Cursor { .. } => "SERVER_CURSOR_CONFIG_INVALID",
            Self::SessionLifetime { .. } => "SERVER_SESSION_LIFETIME_INVALID",
            Self::BindAddress { .. } => "SERVER_ADDRESS_INVALID",
            Self::Listen { .. } => "SERVER_LISTEN_FAILED",
            Self::Serve { .. } => "SERVER_SERVE_FAILED",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[derive(Clone, Default)]
    struct LogCapture(std::sync::Arc<std::sync::Mutex<Vec<u8>>>);
    impl std::io::Write for LogCapture {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    #[test]
    fn raw_session_logs_are_suppressed_even_with_explicit_debug_directives() {
        let logs = LogCapture::default();
        let writer = logs.clone();
        let subscriber = tracing_subscriber::registry()
            .with(tracing_subscriber::EnvFilter::new(
                "trace,tower_sessions::service=trace",
            ))
            .with(
                tracing_subscriber::fmt::layer()
                    .with_ansi(false)
                    .without_time()
                    .with_writer(move || writer.clone())
                    .with_filter(tracing_subscriber::filter::filter_fn(safe_log_target)),
            );
        tracing::subscriber::with_default(subscriber, || {
            tracing::error!(target: "tower_sessions::service", "session secret-fixture");
            tracing::error!(target: "axum_login::service", "login secret-fixture");
            tracing::warn!(target: "tower_sessions_core::session", "record secret-fixture");
            tracing::error!(target: "yydra_http", request_id = "fixture", "request failed");
        });
        let output = String::from_utf8(logs.0.lock().unwrap().clone()).unwrap();
        assert!(!output.contains("secret-fixture"));
        assert_eq!(output.matches("request failed").count(), 1);
        assert!(output.contains("fixture"));
    }
}
