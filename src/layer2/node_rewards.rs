///////////////////////////////////////////////////////////
// my_dex/src/layer2/node_rewards.rs
///////////////////////////////////////////////////////////
//
// Passives Einkommensmodell f�r Nodes 
// Implementierung eines Anreizsystems:
// Nodes erhalten Geb�hren-Anteile proportional zu ihrer Netzwerkleistung 
// (Orderbuchhaltung, Watchtower-Dienste, Online-Zeit)
// Aufbau eines fairen Verteilungsschl�ssels basierend auf Leistungsmessung
// (z.?B. via Prometheus oder einem anderen Metrik-System)

use anyhow::{Result, anyhow};
use std::collections::HashMap;
use tracing::info;
use std::time::Duration;

/// Struktur, die Leistungsmetriken f�r einen Node repr�sentiert.
#[derive(Debug, Clone)]
pub struct NodePerformance {
    pub node_id: String,
    pub orderbook_points: f64,  // Metrik f�r Orderbuchhaltung
    pub watchtower_points: f64, // Metrik f�r Watchtower-Dienste
    pub online_seconds: u64,    // Gemessene Online-Zeit in Sekunden
}

impl NodePerformance {
    /// Berechnet einen Gesamtleistungsscore f�r den Node.
    /// Hier wird eine Beispielgewichtung verwendet:
    /// - Orderbuchhaltung: 50%
    /// - Watchtower-Dienste: 30%
    /// - Online-Zeit: 20% (umgerechnet in Stunden)
    pub fn total_score(&self) -> f64 {
        let online_score = self.online_seconds as f64 / 3600.0; // Umrechnung in Stunden
        0.5 * self.orderbook_points + 0.3 * self.watchtower_points + 0.2 * online_score
    }
}

/// Berechnet die Belohnung f�r jeden Node basierend auf ihren Leistungsscores.
///
/// # Parameter:
/// - `performances`: Eine Liste von NodePerformance, die die Metriken aller Nodes enthalten.
/// - `total_reward_pool`: Der Gesamtbetrag, der verteilt werden soll (in der entsprechenden W�hrungseinheit).
///
/// # R�ckgabe:
/// Eine HashMap, die f�r jeden Node (node_id) den berechneten Belohnungsbetrag enth�lt.
pub fn calculate_node_rewards(
    performances: &[NodePerformance],
    total_reward_pool: u64,
) -> Result<HashMap<String, u64>> {
    // Gesamtscore aller Nodes berechnen
    let total_score: f64 = performances.iter().map(|p| p.total_score()).sum();
    if total_score <= 0.0 {
        return Err(anyhow!("Gesamtscore aller Nodes ist 0 oder negativ."));
    }

    let mut rewards = HashMap::new();
    for p in performances {
        let score = p.total_score();
        // Anteil proportional zum Score berechnen
        let fraction = score / total_score;
        let reward = (fraction * (total_reward_pool as f64)).round() as u64;
        rewards.insert(p.node_id.clone(), reward);
        info!(
            "Node {} erh�lt einen Anteil von {} Einheiten (Score: {:.2}, Anteil: {:.2}%)", 
            p.node_id, reward, score, fraction * 100.0
        );
    }
    Ok(rewards)
}

/// Simuliert die Aktualisierung von Node-Leistungsmetriken.
/// In einer echten Implementierung w�rden diese Daten von einem Metrik-System
/// wie Prometheus gesammelt und ausgewertet.
pub async fn update_node_metrics() -> Vec<NodePerformance> {
    // Beispielhafte, simulierte Metriken:
    vec![
        NodePerformance {
            node_id: "node1".to_string(),
            orderbook_points: 100.0,
            watchtower_points: 80.0,
            online_seconds: 3600 * 24, // 24 Stunden
        },
        NodePerformance {
            node_id: "node2".to_string(),
            orderbook_points: 120.0,
            watchtower_points: 90.0,
            online_seconds: 3600 * 20, // 20 Stunden
        },
        NodePerformance {
            node_id: "node3".to_string(),
            orderbook_points: 90.0,
            watchtower_points: 70.0,
            online_seconds: 3600 * 26, // 26 Stunden
        },
    ]
}

/// Beispiel: F�hrt die Berechnung der Node-Belohnungen durch und gibt die Ergebnisse aus.
/// Diese Funktion w�rde regelm��ig (z.?B. w�chentlich) aufgerufen, um die Belohnungen zu verteilen.
pub async fn distribute_rewards(total_reward_pool: u64) -> Result<()> {
    let metrics = update_node_metrics().await;
    let rewards = calculate_node_rewards(&metrics, total_reward_pool)?;
    for (node_id, reward) in rewards {
        info!("Node {} erh�lt {} Einheiten als Belohnung.", node_id, reward);
    }
    Ok(())
}

/// (Optional) Eine Funktion, die als Hintergrundtask periodisch die Rewards berechnet und verteilt.
/// Hier k�nnte man einen Zeitplan implementieren (z.?B. einmal pro Woche).
pub async fn start_reward_distribution(total_reward_pool: u64, interval_secs: u64) -> Result<()> {
    let mut interval = tokio::time::interval(Duration::from_secs(interval_secs));
    loop {
        interval.tick().await;
        info!("Starte Belohnungsverteilung f�r Nodes...");
        distribute_rewards(total_reward_pool).await?;
    }
}
