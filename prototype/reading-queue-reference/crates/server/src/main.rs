use std::env;
use std::net::SocketAddr;

use sqlx::postgres::PgPoolOptions;
use tower_http::{cors::CorsLayer, trace::TraceLayer};
use tracing::info;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let database_url = env::var("DATABASE_URL")?;
    let pool = PgPoolOptions::new()
        .max_connections(8)
        .connect(&database_url)
        .await?;
    product_persistence_postgres::check_migration_state(&pool).await?;

    let state = product_transport_http::AppState::new(pool);
    let (router, _) = product_transport_http::router()
        .with_state::<()>(state)
        .split_for_parts();
    let app = router
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http());
    let address: SocketAddr = "127.0.0.1:4000".parse()?;
    let listener = tokio::net::TcpListener::bind(address).await?;
    info!(%address, "serving Product Workspace");
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    Ok(())
}

async fn shutdown_signal() {
    let _ = tokio::signal::ctrl_c().await;
}
