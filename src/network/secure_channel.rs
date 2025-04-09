//////////////////////////////////////////////////
/// my_DEX/src/network/secure_channel.rs
//////////////////////////////////////////////////

use tokio::net::TcpStream;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use snow::{Builder, params::NoiseParams, Session};
use bytes::{Bytes, BytesMut};
use std::sync::{Arc, Mutex};
use crate::network::security_monitor::SecurityMonitor; // ? Verbindung mit Security Monitor

pub struct SecureChannel {
    session: Arc<Mutex<Session>>,
    stream: TcpStream,
    monitor: Arc<SecurityMonitor>, // ? Security Monitoring direkt integriert
}

impl SecureChannel {
    pub async fn connect(addr: &str, noise_params: &str, monitor: Arc<SecurityMonitor>) -> Result<Self> {
        let params: NoiseParams = noise_params.parse()?;
        let builder = Builder::new(params);
        let mut session = builder.clone().build_initiator()?;
        let mut stream = TcpStream::connect(addr).await?;
        let mut buf = vec![0u8; 1024];

        let len = session.write_message(&[], &mut buf)?;
        stream.write_all(&buf[..len]).await?;
        let n = stream.read(&mut buf).await?;
        session.read_message(&buf[..n], &mut vec![])?;

        // ?? Noise-Handshake �berwachen
        monitor.monitor_noise_handshake(&session);

        println!("?? Secure Channel aufgebaut mit {}", addr);
        Ok(SecureChannel {
            session: Arc::new(Mutex::new(session)),
            stream,
            monitor,
        })
    }

    pub async fn accept(listener: &TcpListener, noise_params: &str, monitor: Arc<SecurityMonitor>) -> Result<Self> {
        let (mut stream, _addr) = listener.accept().await?;
        let params: NoiseParams = noise_params.parse()?;
        let builder = Builder::new(params);
        let mut session = builder.build_responder()?;
        let mut buf = vec![0u8; 1024];

        let n = stream.read(&mut buf).await?;
        session.read_message(&buf[..n], &mut vec![])?;
        let mut out_buf = vec![0u8; 1024];
        let len = session.write_message(&[], &mut out_buf)?;
        stream.write_all(&out_buf[..len]).await?;

        // ?? Noise-Handshake �berwachen
        monitor.monitor_noise_handshake(&session);

        println!("? Secure Channel akzeptiert Verbindung.");
        Ok(SecureChannel {
            session: Arc::new(Mutex::new(session)),
            stream,
            monitor,
        })
    }

    pub async fn send(&mut self, plaintext: &[u8]) -> Result<()> {
        let nonce = SecurityMonitor::generate_nonce();
        let mut buf = BytesMut::with_capacity(plaintext.len() + 16 + 8);
        buf[..8].copy_from_slice(&nonce.to_le_bytes());

        let len = self.session.lock().unwrap()
            .write_message(plaintext, &mut buf[8..])
            .map_err(|e| {
                self.monitor.log_event("? Fehler bei der Verschl�sselung!");
                anyhow!("?? Verschl�sselung fehlgeschlagen: {:?}", e)
            })?;

        self.stream.write_all(&buf[..len + 8]).await?;
        self.monitor.log_event(&format!("?? Nachricht gesendet mit Nonce: {}", nonce));
        Ok(())
    }

    pub async fn receive(&mut self) -> Result<Bytes> {
        let mut buf = BytesMut::with_capacity(1032);
        let n = self.stream.read_buf(&mut buf).await?;
        if n < 8 {
            return Err(anyhow!("? Ung�ltige Nachricht (zu kurz)"));
        }

        let nonce = u64::from_le_bytes(buf[..8].try_into().unwrap());

        // ?? Replay-Schutz aktivieren
        if !self.monitor.is_valid_nonce(nonce) {
            self.monitor.log_event("?? Replay-Angriff erkannt! Nachricht blockiert.");
            return Err(anyhow!("? Replay-Angriff erkannt! Nachricht blockiert."));
        }

        let mut out = BytesMut::with_capacity(1024);
        let len = self.session.lock().unwrap()
            .read_message(&buf[8..n], &mut out)
            .map_err(|e| {
                self.monitor.log_event("? Fehler bei der Entschl�sselung!");
                anyhow!("?? Entschl�sselung fehlgeschlagen: {:?}", e)
            })?;

        out.truncate(len);
        self.monitor.log_event(&format!("?? Empfangene Nachricht (Nonce {}): {:?}", nonce, out));
        Ok(out.freeze())
    }
}
