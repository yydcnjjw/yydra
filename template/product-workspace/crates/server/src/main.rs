use std::net::SocketAddr;

use tower_http::{cors::CorsLayer, trace::TraceLayer};
use tracing::info;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let (router, _) = product_transport_http::router().split_for_parts();
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
