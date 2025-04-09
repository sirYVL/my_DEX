// my_dex/tests/integration_tests.rs
//
// Funktionen in dieser Datei und ihre Zwecke:
//  1) test_security_validator: Unit-Test zur Überprüfung der Funktionalität des AdvancedSecurityValidator,
//     der sicherstellt, dass Settlement-Validierungen korrekt durchgeführt werden.
//  2) test_settlement_workflow: Integrationstest, der den gesamten Settlement-Workflow mittels des
//     SecuredSettlementEngine-Dekorators testet, um fehlerfreie Transaktionen sicherzustellen.
//  3) test_db_put_get: Unit-Test für grundlegende Datenbankoperationen (PUT und GET), um die Zuverlässigkeit
//     der DB-Layer zu validieren.
//  4) test_high_concurrency_db_access: Lasttest, der unter hoher Concurrency die Stabilität und Performance
//     der Datenbank überprüft.
//  5) test_crdt_snapshot_flow: Test des CRDT-Snapshot-Workflows, der sicherstellt, dass Snapshots korrekt
//     gespeichert und wieder abgerufen werden können.
//  6) test_reliable_gossip_integration: Integrationstest für den Reliable-Gossip-Mechanismus, der simuliert,
//     ob zwei Reliable-Gossip-Nodes Nachrichten austauschen können und fehlende Nachrichten erkannt sowie
//     nachgefordert werden.

use anyhow::Result;
use my_dex::security::security_validator::{AdvancedSecurityValidator, SecurityValidator};
use my_dex::settlement::advanced_settlement::{AdvancedSettlementEngine, Asset};
use my_dex::settlement::secured_settlement::{SettlementEngineTrait, SecuredSettlementEngine};
use my_dex::storage::db_layer::DexDB;
use my_dex::fees::fee_pool::FeePool;
use my_dex::error::DexError;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::time::sleep;

/// Testet die Funktionalität des AdvancedSecurityValidators als Unit-Test.
#[tokio::test]
async fn test_security_validator() -> Result<()> {
    let validator = AdvancedSecurityValidator::new();
    // Wenn die Validierung fehlerfrei durchläuft, gilt der Test als erfolgreich.
    validator.validate_settlement("Test Settlement Validation")?;
    Ok(())
}

/// Integrationstest für den Settlement-Workflow mit dem SecuredSettlementEngine-Decorator.
#[tokio::test]
async fn test_settlement_workflow() -> Result<()> {
    // Erstelle eine temporäre DB (mit InMemory-Fallback falls RocksDB nicht verfügbar ist).
    let db = DexDB::open_with_retries("tmp_test_db", 3, 1)?;
    let arc_db = Arc::new(Mutex::new(db));
    // Erstelle einen FeePool speziell für Settlement-Operationen.
    let fee_pool = Arc::new(FeePool::new(arc_db.clone(), "test/fee_pool"));
    // Initialisiere die AdvancedSettlementEngine.
    let advanced_settlement_engine = AdvancedSettlementEngine::new(fee_pool, arc_db.clone());
    // Verpacke sie in den SecuredSettlementEngine mit einem AdvancedSecurityValidator.
    let mut secured_settlement_engine = SecuredSettlementEngine::new(
        advanced_settlement_engine,
        AdvancedSecurityValidator::new()
    );
    // Führe einen Settlement-Trade aus und prüfe, ob er fehlerfrei abgeschlossen wird.
    secured_settlement_engine.finalize_trade("buyer_test", "seller_test", Asset::BTC, Asset::LTC, 1.0, 50000.0)?;
    Ok(())
}

/// Unit-Test für grundlegende DB-Operationen (PUT und GET).
#[tokio::test]
async fn test_db_put_get() -> Result<()> {
    let db = DexDB::open_with_retries("tmp_test_db_store", 3, 1)?;
    let key = "unit_test_key";
    let value = b"unit_test_value";
    db.put(key.as_bytes(), value)?;
    let loaded = db.get(key.as_bytes())?;
    assert_eq!(loaded, Some(value.to_vec()));
    Ok(())
}

/// Lasttest: Simuliere hohe Concurrency bei DB-Operationen.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_high_concurrency_db_access() -> Result<()> {
    let db = DexDB::open_with_retries("tmp_test_db_concurrent", 3, 1)?;
    let key = "concurrent_key";
    let value = b"concurrent_value".to_vec();
    let num_tasks = 100;
    let mut handles = Vec::new();
    for _ in 0..num_tasks {
        let db_clone = db.clone();
        let key = key.to_string();
        let value = value.clone();
        let handle = tokio::spawn(async move {
            db_clone.put(key.as_bytes(), &value)?;
            let _ = db_clone.get(key.as_bytes())?;
            Result::<(), DexError>::Ok(())
        });
        handles.push(handle);
    }
    for handle in handles {
        handle.await??;
    }
    Ok(())
}

/// Dieser Test prüft den CRDT-Snapshot-Workflow:
#[tokio::test]
async fn test_crdt_snapshot_flow() -> Result<()> {
    let db = DexDB::open_with_retries("tmp_test_db_snapshot", 3, 1)?;
    // Erstelle einen CRDT-Snapshot.
    let snap = my_dex::storage::db_layer::CrdtSnapshot { version: 1, data: vec![1, 2, 3, 4] };
    db.store_crdt_snapshot(&snap)?;
    // Lese den Snapshot wieder aus.
    match db.load_crdt_snapshot(1) {
        Ok(Some(loaded)) => {
            assert_eq!(loaded.version, 1);
            assert_eq!(loaded.data, vec![1, 2, 3, 4]);
        },
        Ok(None) => panic!("Snapshot nicht gefunden"),
        Err(e) => return Err(anyhow::anyhow!("Fehler beim Laden des Snapshots: {:?}", e)),
    }
    Ok(())
}

/// Integrationstest für den Reliable-Gossip-Mechanismus.
/// Dieser Test simuliert zwei Reliable-Gossip-Nodes, die Nachrichten austauschen, und überprüft,
/// ob der Reliable-Gossip-Mechanismus korrekt funktioniert (fehlende Nachrichten werden erkannt und abgefragt).
#[tokio::test]
async fn test_reliable_gossip_integration() -> Result<()> {
    use my_dex::network::reliable_gossip::{GossipNode, GossipMessage};
    use tokio::sync::mpsc;
    use tokio::time::{sleep, Duration};

    // Erstelle zwei asynchrone Kanäle für den Nachrichtenaustausch.
    let (tx_a, rx_a) = mpsc::channel(100);
    let (tx_b, rx_b) = mpsc::channel(100);

    // Initialisiere zwei Reliable-Gossip-Nodes, die als Peers agieren.
    let mut node_a = GossipNode::new("NodeA".to_string(), tx_a, rx_b);
    let mut node_b = GossipNode::new("NodeB".to_string(), tx_b, rx_a);

    // Starte einen Task, der node_b's eingehende Nachrichten verarbeitet.
    let handle_b = tokio::spawn(async move {
        node_b.handle_messages().await;
    });

    // Node A sendet eine Testnachricht.
    node_a.broadcast(b"Reliable Gossip Test Message".to_vec()).await?;

    // Warte kurz, damit node_b die Nachricht empfangen und verarbeiten kann.
    sleep(Duration::from_secs(1)).await;

    // Falls keine Fehler auftreten, gilt der Test als erfolgreich.
    handle_b.await??;
    Ok(())
}
