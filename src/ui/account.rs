/////////////////////////////////////////////
/// my_DEX/src/ui/account.rs
/////////////////////////////////////////////


use axum::{
    Router,
    routing::{get, post},
    extract::State,
    Json,
    response::Html,
};
use std::sync::Arc;
use tokio::sync::Mutex;
use axum::http::StatusCode;
use std::sync::atomic::{AtomicBool, Ordering};
use serde_json::json;
use crate::auth::AuthManager;
use crate::crypto_scraper::{CryptoScraper, PriceFeed};

/// ScraperState kapselt den aktuellen Zustand des Scrapers
/// und ein Flag, das angibt, ob gerade ein Login-/Logout-Prozess läuft.
pub struct ScraperState {
    pub is_processing: AtomicBool,
    pub scraper: Option<CryptoScraper>,
}

impl ScraperState {
    pub fn new() -> Self {
        Self {
            is_processing: AtomicBool::new(false),
            scraper: None,
        }
    }
}

/// Handler für den Login: Initialisiert einen neuen CryptoScraper
/// und markiert den Login-Prozess als aktiv.
async fn handle_login(
    State((auth, scraper_state, price_feed)): State<(
        Arc<AuthManager>,
        Arc<Mutex<ScraperState>>,
        Arc<Mutex<PriceFeed>>
    )>
) -> Result<Json<&'static str>, (StatusCode, String)> {
    auth.handle_login().await;
    let mut state = scraper_state.lock().await;

    if state.is_processing.load(Ordering::SeqCst) {
        return Err((StatusCode::CONFLICT, "A login/logout process is already in progress".into()));
    }
    if state.scraper.is_some() {
        return Err((StatusCode::CONFLICT, "Already logged in".into()));
    }

    state.is_processing.store(true, Ordering::SeqCst);
    match CryptoScraper::new(price_feed.clone()).await {
        Ok(new_scraper) => {
            state.scraper = Some(new_scraper);
            log::info!("User logged in and scraper initialized");
            state.is_processing.store(false, Ordering::SeqCst);
            Ok(Json("Login successful"))
        },
        Err(e) => {
            state.is_processing.store(false, Ordering::SeqCst);
            log::error!("Failed to initialize scraper: {:?}", e);
            Err((StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to initialize scraper: {}", e)))
        }
    }
}

/// Handler für den Logout: Beendet den aktiven CryptoScraper und setzt den Zustand zurück.
async fn handle_logout(
    State((auth, scraper_state, _)): State<(
        Arc<AuthManager>,
        Arc<Mutex<ScraperState>>,
        Arc<Mutex<PriceFeed>>
    )>
) -> Result<Json<&'static str>, (StatusCode, String)> {
    auth.handle_logout().await;
    let mut state = scraper_state.lock().await;
    
    if state.is_processing.load(Ordering::SeqCst) {
        return Err((StatusCode::CONFLICT, "A login/logout process is already in progress".into()));
    }
    if let Some(mut scraper_instance) = state.scraper.take() {
        state.is_processing.store(true, Ordering::SeqCst);
        // Versuche, den Browser sauber herunterzufahren
        if let Some(client) = scraper_instance.client.take() {
            if let Err(e) = client.close().await {
                log::error!("Error closing browser: {:?}", e);
            }
        }
        log::info!("User logged out and scraper shut down");
        state.is_processing.store(false, Ordering::SeqCst);
        Ok(Json("Logout successful"))
    } else {
        Err((StatusCode::BAD_REQUEST, "No active scraper session".to_string()))
    }
}

/// Liefert die aktuellen Preise aus dem PriceFeed als JSON.
async fn get_prices(
    State(price_feed): State<Arc<Mutex<PriceFeed>>>
) -> Result<Json<PriceFeed>, (StatusCode, String)> {
    let data = price_feed.lock().await.clone();
    Ok(Json(data))
}

/// Zeigt eine einfache Portfolio-Webseite an, die die aktuellen Preise auflistet.
async fn account_ui(
    State(price_feed): State<Arc<Mutex<PriceFeed>>>
) -> Html<String> {
    let data = price_feed.lock().await;
    Html(format!(
        r#"
        <!DOCTYPE html>
        <html>
        <head>
            <meta charset="utf-8">
            <title>Your Portfolio</title>
        </head>
        <body>
            <h1>Your Portfolio</h1>
            <ul>
                {}
            </ul>
        </body>
        </html>
        "#,
        data.prices.iter()
            .map(|(k, v)| format!("<li>{}: {}</li>", k, v))
            .collect::<String>()
    ))
}

/// Erzeugt einen Router, der alle Account-Endpunkte enthält.
pub fn account_routes() -> Router {
    Router::new()
        .route("/api/login", post(handle_login))
        .route("/api/logout", post(handle_logout))
        .route("/api/prices", get(get_prices))
        .route("/account", get(account_ui))
}
