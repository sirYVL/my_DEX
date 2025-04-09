///////////////////////////////////////////
// my_dex/src/network/p2p_operations.rs

use tokio::net::TcpStream;
use tokio_rustls::TlsConnector;
use rustls::ClientConfig;
use std::sync::Arc;
use webpki::DnsNameRef;
use tokio::io::{AsyncWriteExt, AsyncReadExt};
use anyhow::{Result, Context};
use tracing::{info, error};

/// Stellt eine TLS-verschl�sselte TCP-Verbindung zu einem Peer her,
/// sendet Daten und liest eine Antwort.
/// 
/// # Parameter
/// - `peer_addr`: Die IP-Adresse und Port des Peers, z.B. "192.168.1.100:443".
/// - `domain`: Der Domain-Name des Peers, der zur TLS-Validierung genutzt wird.
/// - `data`: Die zu sendenden Daten als Byte-Slice.
///
/// # R�ckgabe
/// Gibt ein `Vec<u8>` mit der Antwort des Peers zur�ck.
pub async fn send_secure_data_to_peer(peer_addr: &str, domain: &str, data: &[u8]) -> Result<Vec<u8>> {
    // Erstelle einen leeren Root-Zertifikatsspeicher.
    let mut root_cert_store = rustls::RootCertStore::empty();

    // F�ge die Standard-Root-Zertifikate hinzu (aus webpki_roots).
    for cert in webpki_roots::TLS_SERVER_ROOTS.0.iter() {
        root_cert_store.add(cert).context("Failed to add root certificate")?;
    }

    // Konfiguriere den TLS-Client ohne Client-Authentifizierung.
    let config = ClientConfig::builder()
        .with_safe_defaults()
        .with_root_certificates(root_cert_store)
        .with_no_client_auth();

    let connector = TlsConnector::from(Arc::new(config));

    // Baue eine TCP-Verbindung zum Peer auf.
    let stream = TcpStream::connect(peer_addr)
        .await
        .with_context(|| format!("Failed to connect to peer at {}", peer_addr))?;

    // Konvertiere den Domain-Namen in ein DnsNameRef f�r die TLS-Validierung.
    let dnsname = DnsNameRef::try_from_ascii_str(domain)
        .context("Invalid DNS name")?;

    // Stelle die TLS-Verbindung her.
    let mut tls_stream = connector.connect(dnsname, stream)
        .await
        .context("TLS connection failed")?;

    // Sende die Daten �ber die verschl�sselte Verbindung.
    tls_stream.write_all(data)
        .await
        .context("Failed to write data to TLS stream")?;
    tls_stream.flush()
        .await
        .context("Failed to flush TLS stream")?;

    // Lese die Antwort vom Peer.
    let mut response = Vec::new();
    tls_stream.read_to_end(&mut response)
        .await
        .context("Failed to read response from TLS stream")?;

    info!("Secure communication with peer at {} successful", peer_addr);
    Ok(response)
}
