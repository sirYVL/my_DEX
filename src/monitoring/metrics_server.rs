// my_dex/src/monitoring/metrics_server.rs

use axum::{routing::get, Router, response::IntoResponse, http::StatusCode};
use crate::monitoring::global_metrics::gather_global_metrics;
use crate::monitoring::local_metrics::gather_local_metrics;

async fn global_metrics_handler() -> impl IntoResponse {
    let metrics = gather_global_metrics();
    (StatusCode::OK, metrics)
}

async fn local_metrics_handler() -> impl IntoResponse {
    let metrics = gather_local_metrics();
    (StatusCode::OK, metrics)
}

pub async fn run_metrics_server() {
    let app = Router::new()
        .route("/metrics/global", get(global_metrics_handler))
        .route("/metrics/local", get(local_metrics_handler));

    axum::Server::bind(&"0.0.0.0:9100".parse().unwrap())
        .serve(app.into_make_service())
        .await
        .unwrap();
}
