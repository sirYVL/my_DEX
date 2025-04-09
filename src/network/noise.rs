// my_dex/src/network/noise.rs
//
// Sichere Kommunikation über Noise-Protokoll. 
// Bindet snow::Session an TCP/QUIC-Stream, 
// generiert ephemeral Keys und führt Handshake durch.
//
// In einer realen DEX würdest du an passender Stelle 
// Node-Identitäten oder ephemeral Key austauschen.

use std::io::{Read, Write};
use std::net::TcpStream;
use anyhow::{Result, anyhow};
use snow::{Builder, params::NoiseParams, Session};
use tracing::{info, warn, error};

/// NoiseSession => enthält aktiven snow::Session, plus Info zum Modus
pub struct NoiseSession {
    pub session: Session,
    pub is_initiator: bool,
}

impl NoiseSession {
    /// Erzeugt eine Initiator-Session => wir bauen "NX" oder "XX" etc.
    pub fn initiator(noise_params: NoiseParams) -> Result<Self> {
        let builder = Builder::new(noise_params);
        let session = builder.build_initiator().map_err(|e| anyhow!(e))?;
        Ok(NoiseSession { session, is_initiator: true })
    }

    /// Erzeugt eine Responder-Session
    pub fn responder(noise_params: NoiseParams) -> Result<Self> {
        let builder = Builder::new(noise_params);
        let session = builder.build_responder().map_err(|e| anyhow!(e))?;
        Ok(NoiseSession { session, is_initiator: false })
    }

    /// Führt Handshake mit einem Stream (TCP oder QUIC) durch.
    /// Hier Beispiel: TCP
    pub fn handshake_tcp(&mut self, mut stream: &TcpStream) -> Result<()> {
        // 1) Write first message (wenn Initiator)
        if self.is_initiator {
            let mut buf = vec![0u8; 65535];
            let len = self.session.write_message(&[], &mut buf)
                .map_err(|e| anyhow!("write_message: {}", e))?;
            stream.write_all(&buf[..len])?;
        }

        // 2) Read response
        {
            let mut read_buf = [0u8; 65535];
            let n = stream.read(&mut read_buf)?;
            let mut out = vec![0u8; 65535];
            let len_resp = self.session.read_message(&read_buf[..n], &mut out)
                .map_err(|e| anyhow!("read_message: {}", e))?;
            out.truncate(len_resp);
            // Option: if next step needed, write back ...
            if !self.is_initiator {
                // wir als Responder => schreib + finalize
                let mut send_buf = vec![0u8; 65535];
                let len2 = self.session.write_message(&[], &mut send_buf)
                    .map_err(|e| anyhow!("responder write: {}", e))?;
                stream.write_all(&send_buf[..len2])?;
            }
        }

        info!("Noise handshake successful");
        Ok(())
    }

    /// Verschlüsselte Send-Funktion
    pub fn send(&mut self, stream: &TcpStream, plaintext: &[u8]) -> Result<()> {
        let mut buf = vec![0u8; plaintext.len() + 128];
        let len = self.session.write_message(plaintext, &mut buf)
            .map_err(|e| anyhow!("send write_message: {}", e))?;
        stream.write_all(&buf[..len])?;
        Ok(())
    }

    /// Verschlüsselte Receive-Funktion
    pub fn receive(&mut self, stream: &TcpStream) -> Result<Vec<u8>> {
        let mut read_buf = [0u8; 65535];
        let n = stream.read(&mut read_buf)?;
        if n == 0 {
            return Err(anyhow!("stream closed"));
        }
        let mut out = vec![0u8; 65535];
        let len = self.session.read_message(&read_buf[..n], &mut out)
            .map_err(|e| anyhow!("receive read_message: {}", e))?;
        out.truncate(len);
        Ok(out)
    }
}

/// Externe Hilfsfunktion: "perform_noise_handshake"
/// Falls du an der alten Signatur festhalten willst.
pub async fn perform_noise_handshake() -> Result<()> {
    // z. B. Initiator
    let noise_params: NoiseParams = "Noise_NX_25519_ChaChaPoly_SHA256".parse()?;
    let mut initiator = NoiseSession::initiator(noise_params)?;
    // In echt: connect TCP
    let stream = TcpStream::connect("127.0.0.1:9443")?;
    initiator.handshake_tcp(&stream)?;
    info!("Noise Handshake abgeschlossen (Initiator).");
    Ok(())
}
