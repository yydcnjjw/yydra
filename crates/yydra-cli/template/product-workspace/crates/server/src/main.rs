// SPDX-License-Identifier: MIT OR Apache-2.0

use std::env;
use std::net::SocketAddr;

use product_application::{HealthService, ReadingQueueService};
use product_persistence_postgres::Database;
use tracing::info;
use yydra_auth::{AuthConfig, AuthService};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let database_url = env::var("DATABASE_URL")?;
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
            return Err(
                "GITHUB_CLIENT_ID and GITHUB_CLIENT_SECRET must be configured together".into(),
            );
        }
    }
    if let Ok(seconds) = env::var("YYDRA_AUTH_SESSION_SECONDS") {
        config = config.lifetime_seconds(seconds.parse()?)?;
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
            if cleanup.cleanup().await.is_err() {
                tracing::warn!("authentication expiry cleanup failed");
            }
        }
    });
    let cursor_signing_key = env::var("YYDRA_READING_QUEUE_CURSOR_SIGNING_KEY")?;

    let app = authentication.layer(product_transport_http::router(
        HealthService::new(database.clone()),
        ReadingQueueService::new(database, cursor_signing_key.as_bytes())?,
    ));
    #[cfg(feature = "auth-fixture")]
    let app = app.merge(yydra_auth::test_provider::router());
    let address: SocketAddr = env::var("YYDRA_BIND_ADDRESS")
        .unwrap_or_else(|_| "127.0.0.1:4000".to_owned())
        .parse()?;
    let listener = tokio::net::TcpListener::bind(address).await?;
    info!(%address, "serving Product Workspace");
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;
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
