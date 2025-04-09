// my_dex/src/kademlia/mdns_discovery.rs
//
// Produktionsreife mDNS-Lösung:
//  - Der Responder announct sich dauerhaft
//  - Wir lauschen auf Discovery-Events (Discovered/Lost)
//  - Gefundene Peers werden in Kademlia eingetragen
//  - Dieses Modul läuft in einem eigenen Task, damit es parallel arbeitet.
//
// (c) Ihr DEX-Projekt – modifiziert für Produktionsnähe.

use libmdns::{Responder, ServiceName, Event, ServiceDiscovery};
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use tokio::task;
use tokio::sync::mpsc;
use tracing::{info, debug, warn, error};
use anyhow::Result;

// Hier importieren wir unser KademliaService + NodeId:
use crate::kademlia::kademlia_service::{KademliaService, NodeId};

/// Einfache Konfiguration für mDNS:
/// - `service_name` ist typischerweise "_mydex._udp" oder "_mydex._udp.local"
/// - `port` ist der lokale Port, auf dem Dein Node erreichbar ist (z. B. 9000)
#[derive(Debug, Clone)]
pub struct MdnsConfig {
    /// z. B. "_mydex._udp" (ohne ".local", s. unten).
    pub service_name: String,
    /// Der UDP/TCP-Port (z. B. 9000)
    pub port: u16,
}

/// Startet einen mDNS-Responder und lauscht auf Discovery-Events.
/// - Announced den eigenen Service
/// - Alle gefundenen Peers werden dem KademliaService hinzugefügt.
///
/// Dieser Task läuft dauerhaft im Hintergrund.
/// Beispielaufruf in `main.rs`:
/// ```ignore
/// let kademlia_arc = Arc::new(Mutex::new(my_kademlia_service));
/// tokio::spawn(async move {
///     if let Err(e) = start_mdns_discovery(kademlia_arc, MdnsConfig {...}).await {
///         eprintln!("mDNS error: {:?}", e);
///     }
/// });
/// ```
pub async fn start_mdns_discovery(
    kademlia: Arc<Mutex<KademliaService>>,
    config: MdnsConfig
) -> Result<()> 
{
    // 1) Erzeuge Responder (mDNS-Server), der unseren Service announct
    let responder = Responder::spawn()
        .map_err(|e| anyhow::anyhow!("mDNS Responder spawn failed: {:?}", e))?;

    // 2) Parse den ServiceName. 
    //    Z. B. wenn man "service_name"="mydexnode._mydex._udp.local", 
    //    kann man ServiceName::new(...) anwenden. 
    //    Alternativ "mydexnode" als Instanz, und "_mydex._udp.local" als 
    //    service. Hier vereinfachen wir:
    //
    //    Achte darauf, dass du im DNS-Sense:
    //      <instance>.<service> <=> "my-node._mydex._udp.local"
    //    Hier machen wir "mydexnode" als Instanz:
    let full_service = format!("{}.local", config.service_name);
    let service_name = ServiceName::new(&full_service)
        .map_err(|e| anyhow::anyhow!("Invalid mDNS service name '{}': {:?}", full_service, e))?;

    // 3) Registriere dich: 
    //    - "mydexnode" => Instanz-Name
    //    - "config.port" => Port
    let instance_name = "mydexnode"; 
    let _svc_registration = responder.register(
        service_name,
        instance_name,
        config.port,
        &[] // hier könnte man optional Key=Value für TXTRecords angeben
    );
    
    info!("mDNS: Service announced => name='{}', instance='{}', port={}", config.service_name, instance_name, config.port);

    // 4) Ein "receiver", der mDNS-Events empfängt (Discovered, Lost, etc.)
    let mut receiver = responder.listen();

    // 5) Wir spawnen einen Task, der alle ankommenden Events auswertet
    task::spawn(async move {
        while let Some(event) = receiver.recv().await {
            match event {
                Event::ServiceDiscovery(sd_event) => {
                    match sd_event {
                        ServiceDiscovery::Discovered(info) => {
                            // "Discovered" => info host_name, addr, port
                            debug!("mDNS => discovered: {:?}", info);

                            if let Some(ip) = info.addr {
                                let discovered_port = info.port;
                                let sock = SocketAddr::new(ip, discovered_port);

                                // Da wir nicht wissen, welche NodeId der Peer hat, generieren wir Dummy:
                                let node_id = NodeId::random();

                                // In KademliaService eintragen
                                let mut kad = kademlia.lock().unwrap();
                                kad.table.update_node(node_id, sock);

                                debug!(
                                    "mDNS => Inserted discovered peer => Kademlia: node_id=({:02x?}), sock={}",
                                    &node_id.0[..4], sock
                                );
                            }
                        }
                        ServiceDiscovery::Lost(info) => {
                            // Falls gewünscht: bei Lost => remove_node in Kademlia
                            debug!("mDNS => lost: {:?}", info);
                        }
                    }
                }
                other_event => {
                    debug!("mDNS => other event: {:?}", other_event);
                }
            }
        }
    });

    // => Dieser Aufruf kehrt direkt zurück, d. h. der Task läuft 
    //    unabhängig weiter, bis das Programm endet oder Responder droppt.
    // => Falls man einen Endlos-Block will => hier nicht nötig, 
    //    da 'responder.listen()' im Task endlos läuft.

    Ok(())
}
