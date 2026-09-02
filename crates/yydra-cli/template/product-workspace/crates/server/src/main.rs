// SPDX-License-Identifier: MIT OR Apache-2.0

use std::env;
use std::net::SocketAddr;

use product_application::{HealthService, ReadingQueueService};
use product_persistence_postgres::Database;
use product_transport_http::BearerAuthentication;
use tower_http::{cors::CorsLayer, trace::TraceLayer};
use tracing::info;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let database_url = env::var("DATABASE_URL")?;
    let database = Database::connect(&database_url, 4).await?;
    database.verify_compiled_migrations().await?;
    let authentication = BearerAuthentication::new(
        env::var("YYDRA_AUTH_CONTRACT_TOKEN")
            .unwrap_or_else(|_| "local-framework-contract".to_owned()),
        env::var("YYDRA_AUTH_CONTRACT_FORBIDDEN_TOKEN")
            .unwrap_or_else(|_| "local-framework-forbidden".to_owned()),
    )?;
    let cursor_signing_key = env::var("YYDRA_READING_QUEUE_CURSOR_SIGNING_KEY")?;

    let app = product_transport_http::router(
        HealthService::new(database.clone()),
        ReadingQueueService::new(database, cursor_signing_key.as_bytes())?,
        authentication,
    )
    .layer(CorsLayer::permissive())
    .layer(TraceLayer::new_for_http());
    let address: SocketAddr = env::var("YYDRA_BIND_ADDRESS")
        .unwrap_or_else(|_| "127.0.0.1:4000".to_owned())
        .parse()?;
    let listener = tokio::net::TcpListener::bind(address).await?;
    info!(%address, "serving Product Workspace");
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;
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
