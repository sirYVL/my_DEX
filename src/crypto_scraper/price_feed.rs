////////////////////////////////////////
// my_dex/src/crypto_scraper/price_feed.rs
////////////////////////////////////////

use anyhow::Result;
use std::collections::HashMap;
use chrono::Utc;
use tokio::time::{sleep, Duration};
use crate::crypto_scraper::stealth_browser;
use fantoccini::Client;
use scraper::{Html, Selector};

#[derive(Debug, Clone)]
pub struct PriceFeed {
    pub prices: HashMap<String, String>,
    pub last_updated: i64,
}

impl PriceFeed {
    pub fn new() -> Self {
        Self {
            prices: HashMap::new(),
            last_updated: Utc::now().timestamp(),
        }
    }
}

/// Extrahiert Kursdaten von der Ziel-Webseite mithilfe des �bergebenen Clients.
/// Hier musst du den spezifischen Code einf�gen, um die Kursdaten aus dem HTML zu parsen.
async fn extract_prices(client: &Client) -> Result<HashMap<String, String>> {
    // Beispiel: Navigiere zu einer URL, die Kursdaten enth�lt.
    client.goto("https://example.com/crypto-prices").await?;
    // Warte, bis die Seite vollst�ndig geladen ist
    sleep(Duration::from_secs(5)).await;
    
    // Hole den Seitenquelltext
    let page_source = client.source().await?;
    // Parst den HTML-Inhalt
    let document = Html::parse_document(&page_source);
    // Beispiel-Selektor, passe diesen an deine Zielseite an
    let selector = Selector::parse(".price-item").unwrap();
    let mut prices = HashMap::new();

    // Iteriere �ber alle Elemente, die mit dem Selektor �bereinstimmen
    for element in document.select(&selector) {
        let coin = element.value().attr("data-coin").unwrap_or("unknown").to_string();
        let price = element.text().collect::<Vec<_>>().join("").trim().to_string();
        prices.insert(coin, price);
    }

    Ok(prices)
}

/// F�hrt das PriceFeed-Update-System aus, das periodisch den Stealth-Browser verwendet,
/// um die aktuellen Kursdaten abzurufen und den PriceFeed zu aktualisieren.
pub async fn run_price_feed_system(price_feed: std::sync::Arc<tokio::sync::Mutex<PriceFeed>>) -> Result<()> {
    // Initialisiere den Stealth-Browser
    let client = stealth_browser::setup_stealth_browser().await?;
    
    loop {
        // Extrahiere die Kursdaten
        let prices = extract_prices(&client).await?;
        
        // Aktualisiere den PriceFeed-Zustand
        {
            let mut pf = price_feed.lock().await;
            pf.prices = prices;
            pf.last_updated = Utc::now().timestamp();
        }
        
        // Warte eine Minute, bevor die Kurse erneut abgefragt werden
        sleep(Duration::from_secs(60)).await;
    }
}
