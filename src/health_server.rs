////////////////////////////////////////////////////
/// my_dex/src/health_server.rs
////////////////////////////////////////////////////

use axum::{
    routing::get,
    Router,
    response::IntoResponse,
    http::StatusCode,
};

use std::net::SocketAddr;

pub async fn start_health_server(port: u16) {
    // Baue die Routen f�r Readiness und Liveness
    let app = Router::new()
        .route("/healthz", get(liveness))
        .route("/readyz", get(readiness));

    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    println!("Starting health server on {}", addr);

    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await
        .unwrap();
}

// Liveness Probe
async fn liveness() -> impl IntoResponse {
    StatusCode::OK
}

// Readiness Probe (optional Logik erweiterbar)
async fn readiness() -> impl IntoResponse {
    StatusCode::OK
}
