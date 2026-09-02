// SPDX-License-Identifier: MIT OR Apache-2.0

use std::env;
use std::net::SocketAddr;

use product_application::{HealthService, ReadingQueueService};
use product_persistence_postgres::Database;
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

    let app = product_transport_http::router(
        HealthService::new(database.clone()),
        ReadingQueueService::new(database),
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
