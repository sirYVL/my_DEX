// my_dex/tests/monitoring_tests.rs

use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::Router;
use tower::ServiceExt; // f�r oneshot

// Angenommen, du hast die Handler in my_dex/src/monitoring/metrics_server.rs definiert:
use my_dex::monitoring::metrics_server::{global_metrics_handler, local_metrics_handler};

#[tokio::test]
async fn test_global_metrics_endpoint() {
    // Simuliere eine Anfrage an den globalen Metriken-Endpunkt
    let response = global_metrics_handler().await.into_response();
    assert_eq!(response.status(), StatusCode::OK);
    // Optional: Hier k�nnte man den Response-Body weiter analysieren.
}

#[tokio::test]
async fn test_local_metrics_endpoint() {
    // Simuliere eine Anfrage an den lokalen Metriken-Endpunkt
    let response = local_metrics_handler().await.into_response();
    assert_eq!(response.status(), StatusCode::OK);
    // Optional: Auch hier den Response-Body pr�fen.
}
