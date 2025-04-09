//////////////////////////////////////////////////
/// my_DEX/src/network/p2p_adapter.rs
/////////////////////////////////////////////////

use std::{
    collections::HashMap,
    net::{SocketAddr, ToSocketAddrs},
    sync::{Arc, Mutex},
    time::Duration,
};

use tokio::{
    net::{TcpListener, TcpStream},
    io::{AsyncReadExt, AsyncWriteExt},
    time::sleep,
    task::JoinHandle,
};
use tracing::{debug, info, warn, error};
use anyhow::{Result, anyhow};

use crate::kademlia::kademlia_service::{KademliaP2PAdapter, KademliaMessage};
use snow::{Builder, params::NoiseParams, Session};
use bincode;

/// Dieses Struct hält die Sitzung für einen Peer:
/// - Der Schreib-Halbzugriff (write_half), um asynchron Daten zu senden.
/// - Ein Noise-Session-Objekt, um sowohl verschlüsselt zu senden als auch
///   in der Gegenrichtung zu entschlüsseln.
///   (Im Lese-Loop haben wir ebenfalls Zugriff auf die Session.)
struct PeerConnection {
    write_half: tokio::net::OwnedWriteHalf,
    noise_session: Session,  // beidseitig => hier z. B. Responder- oder Initiator-Side
}

/// TCP + Noise-XX-Adapter für Kademlia.
/// - Lauscht auf `local_addr`
/// - Verwaltet eine HashMap an aktiven Verbindungen (SocketAddr -> PeerConnection).
/// - Jede eingehende Verbindung durchläuft den Noise-Handshake (Responder).
/// - Jede ausgehende Verbindung durchläuft den Noise-Handshake (Initiator).
/// - Danach werden KademliaMessage binär kodiert (bincode) und via Noise verschlüsselt.
pub struct TcpP2PAdapter {
    local_addr: SocketAddr,
    connections: Arc<Mutex<HashMap<SocketAddr, PeerConnection>>>,
    listener_handle: Arc<Mutex<Option<JoinHandle<()>>>>,
}

impl TcpP2PAdapter {
    /// Erzeugt einen neuen Adapter (ohne sofort zu lauschen).
    /// Nutze `start_listener()` um die eingehenden Verbindungen zu akzeptieren.
    pub fn new(local_addr: SocketAddr) -> Self {
        Self {
            local_addr,
            connections: Arc::new(Mutex::new(HashMap::new())),
            listener_handle: Arc::new(Mutex::new(None)),
        }
    }

    /// Startet den TCP-Listener (Noise-Responder für eingehende) asynchron in einem Tokio-Task.
    /// Jede eingehende Verbindung durchläuft den Noise-Handshake (Responder).
    /// Anschließend wird in einer Endlosschleife in `handle_incoming_loop` 
    /// der verschlüsselte Datenstrom gelesen, deserialisiert und an KademliaService gegeben.
    ///
    /// Da wir im Trait selbst nicht direkt den KademliaService referenzieren, 
    /// müsstest du in einer echten Implementierung hier ggf. 
    /// einen Callback oder mpsc-Sender übergeben, 
    /// um `kad_service.handle_message(remote_addr, msg)` aufzurufen.
    pub fn start_listener(&self) -> Result<()> {
        let local_addr = self.local_addr;
        let connections_clone = self.connections.clone();

        let mut guard = self.listener_handle.lock().unwrap();
        if guard.is_some() {
            warn!("Listener bereits gestartet, ignoriere zweiten Aufruf.");
            return Ok(());
        }

        let handle = tokio::spawn(async move {
            let listener = match TcpListener::bind(local_addr).await {
                Ok(l) => {
                    info!("TcpP2PAdapter + Noise => Listening on {}", local_addr);
                    l
                }
                Err(e) => {
                    error!("Bind-Error => {}: {:?}", local_addr, e);
                    return;
                }
            };

            loop {
                let (socket, remote_addr) = match listener.accept().await {
                    Ok(s) => s,
                    Err(e) => {
                        warn!("Fehler bei accept(): {:?}", e);
                        continue;
                    }
                };
                info!("Eingehende Verbindung von {}", remote_addr);

                let connections_arc = connections_clone.clone();
                // Spawn Task => Noise-Handshake + Lese-Loop
                tokio::spawn(async move {
                    if let Err(e) = handle_incoming_connection(socket, remote_addr, connections_arc).await {
                        warn!("Fehler in handle_incoming_connection({}): {:?}", remote_addr, e);
                    }
                });
            }
        });
        *guard = Some(handle);
        Ok(())
    }
}

/// Asynchrones Hilfsfunktion: Noise-Handshake (Responder).
/// Anschließend read-loop -> bincode -> KademliaMessage. 
async fn handle_incoming_connection(
    socket: TcpStream,
    remote_addr: SocketAddr,
    connections_arc: Arc<Mutex<HashMap<SocketAddr, PeerConnection>>>
) -> Result<()> {
    // 1) Noise-Params: wir machen "Noise_XX_25519_ChaChaPoly_SHA256"
    let noise_params: NoiseParams = "Noise_XX_25519_ChaChaPoly_SHA256".parse()
        .map_err(|e| anyhow!("Noise Params parse error: {:?}", e))?;

    let builder = Builder::new(noise_params);
    // Da wir ephemeral (keine statischen Keys) nutzen: 
    // => build_responder
    let mut noise_session = builder
        .build_responder()
        .map_err(|e| anyhow!("build_responder: {:?}", e))?;

    // 2) Socket -> split
    let (mut read_half, write_half) = socket.into_split();

    // 3) Handshake-Phase:
    //    => "Noise_XX" erfordert 3 messages.
    //    => wir (Responder) warten zuerst auf msg von Initiator
    let mut buf = [0u8; 1024];
    let n1 = read_half.read(&mut buf).await?;
    if n1 == 0 {
        return Err(anyhow!("Handshake-Fehler => Remote closed immediately"));
    }
    let mut tmp_out = vec![0u8; 1024];
    let len1 = noise_session.read_message(&buf[..n1], &mut tmp_out)
        .map_err(|e| anyhow!("noise read_message(1): {:?}", e))?;
    debug!("Responder => erstes Handshake-Fragment gelesen ({} bytes).", n1);

    // => Sende 2. msg
    let mut msg2 = vec![0u8; 1024];
    let l2 = noise_session.write_message(&[], &mut msg2)
        .map_err(|e| anyhow!("noise write_message(2): {:?}", e))?;
    // => an remote
    let mut wh = write_half.clone();
    wh.write_all(&msg2[..l2]).await?;
    debug!("Responder => zweites Handshake-Fragment gesendet ({} bytes).", l2);

    // => warte drittes
    let n3 = read_half.read(&mut buf).await?;
    if n3 == 0 {
        return Err(anyhow!("Handshake-Fehler => Remote closed on 3rd msg"));
    }
    let len3 = noise_session.read_message(&buf[..n3], &mut tmp_out)
        .map_err(|e| anyhow!("noise read_message(3): {:?}", e))?;
    debug!("Responder => drittes Handshake-Fragment gelesen ({} bytes).", n3);

    if !noise_session.is_handshake_complete() {
        return Err(anyhow!("Noise-Handshake (XX) nicht komplett => Abbruch."));
    }
    info!("Noise-Responder Handshake erfolgreich => remote={}", remote_addr);

    // 4) Noise-Sitzung => wir packen es in `PeerConnection`.
    let peer_conn = PeerConnection {
        write_half,
        noise_session,
    };

    // 5) in connections-Map packen
    {
        let mut lock = connections_arc.lock().unwrap();
        lock.insert(remote_addr, peer_conn);
    }

    // 6) Lese-Loop => 
    //    - wir warten auf verschlüsselte KademliaMessages
    //    - wir decrypten + bincode-deserialize
    //    - in echtem code: kad_svc.handle_message(remote_addr, msg)
    read_loop_incoming(remote_addr, connections_arc, read_half).await?;

    Ok(())
}

/// Ständiger Lese-Loop nach abgeschlossenem Handshake.
/// Wir holen uns unser PeerConnection aus der Map, um 
/// an die `noise_session` zu gelangen (die wir im Responder init. haben).
async fn read_loop_incoming(
    remote_addr: SocketAddr,
    connections_arc: Arc<Mutex<HashMap<SocketAddr, PeerConnection>>>,
    mut read_half: tokio::net::OwnedReadHalf,
) -> Result<()> {
    let mut buf = [0u8; 4096];
    loop {
        let n = match read_half.read(&mut buf).await {
            Ok(0) => {
                info!("Remote {} => EOF => Closing read_loop", remote_addr);
                break;
            }
            Ok(n) => n,
            Err(e) => {
                warn!("Read-Error bei {} => {:?}", remote_addr, e);
                break;
            }
        };
        // => Aus der Map => noise_session
        let mut guard = connections_arc.lock().unwrap();
        let conn = match guard.get_mut(&remote_addr) {
            Some(c) => c,
            None => {
                warn!("ConnectionState für {} verschwunden => Abbruch read_loop", remote_addr);
                break;
            }
        };
        let mut decrypted_msg = vec![0u8; 4096];
        let len = conn.noise_session.read_message(&buf[..n], &mut decrypted_msg)
            .map_err(|e| anyhow!("Noise decrypt read_message => {:?}", e))?;
        decrypted_msg.truncate(len);

        // => bincode deserialize
        let msg: KademliaMessage = match bincode::deserialize(&decrypted_msg) {
            Ok(m) => m,
            Err(e) => {
                warn!("bincode deserialize => Fehler: {:?}", e);
                break;
            }
        };
        info!("Empfangen (verschlüsselt) von {} => {:?}", remote_addr, msg);

        // => In echtem System: kad_svc.handle_message(remote_addr, msg);
        // => hier: no-op
    }
    // => wir entfernen die Connection:
    {
        let mut guard2 = connections_arc.lock().unwrap();
        guard2.remove(&remote_addr);
    }
    info!("Beende read_loop_incoming for {}", remote_addr);
    Ok(())
}

impl TcpP2PAdapter {
    /// Initiator-Verbindungsaufbau (wenn wir `send_kademlia_msg` an 
    /// unbekannten Peer aufrufen) => Machen den Noise-XX-Handshake als Initiator.
    async fn connect_and_handshake_initiator(
        &self,
        addr: SocketAddr
    ) -> Result<()> {
        // DNS-Auflösung
        let resolved = match addr.to_string().to_socket_addrs() {
            Ok(mut i) => i.next().unwrap_or(addr),
            Err(e) => {
                return Err(anyhow!("DNS-Auflösung fehlgeschlagen => {}", e));
            }
        };
        let stream = TcpStream::connect(resolved).await
            .map_err(|e| anyhow!("connect() zu {} => {:?}", resolved, e))?;

        let (mut read_half, write_half) = stream.into_split();
        let noise_params: NoiseParams = "Noise_XX_25519_ChaChaPoly_SHA256".parse()?;
        let builder = Builder::new(noise_params);
        let mut noise_session = builder.build_initiator()?;

        // Handshake Initiator: 3 Msg
        // 1) Schicke msg1
        let mut msg1 = vec![0u8; 1024];
        let l1 = noise_session.write_message(&[], &mut msg1)
            .map_err(|e| anyhow!("noise write_message(1): {:?}", e))?;
        let mut wh_clone = write_half.clone();
        wh_clone.write_all(&msg1[..l1]).await?;

        // 2) Lese msg2
        let mut buf = [0u8; 1024];
        let n2 = read_half.read(&mut buf).await?;
        if n2 == 0 {
            return Err(anyhow!("Handshake abgebrochen => remote schloss (2)"));
        }
        let mut tmp_out = vec![0u8; 1024];
        noise_session.read_message(&buf[..n2], &mut tmp_out)
            .map_err(|e| anyhow!("noise read_message(2): {:?}", e))?;

        // 3) Schicke msg3
        let mut msg3 = vec![0u8; 1024];
        let l3 = noise_session.write_message(&[], &mut msg3)
            .map_err(|e| anyhow!("noise write_message(3): {:?}", e))?;
        let mut wh_clone2 = write_half.clone();
        wh_clone2.write_all(&msg3[..l3]).await?;

        if !noise_session.is_handshake_complete() {
            return Err(anyhow!("Handshake unvollständig (Initiator) => Abbruch."));
        }
        info!("Noise-Initiator Handshake erfolgreich => remote={}", addr);

        // => Speichere in connections
        let peer_conn = PeerConnection {
            write_half,
            noise_session,
        };
        let mut lock = self.connections.lock().unwrap();
        lock.insert(addr, peer_conn);

        // => Asynchroner read-Loop
        // Wir spawnen analog handle_incoming => 
        //   aber wir haben hier => wir "sind" der Initiator =>  read_loop_incoming
        let connections_clone = self.connections.clone();
        tokio::spawn(async move {
            if let Err(e) = read_loop_incoming(addr, connections_clone, read_half).await {
                warn!("read_loop_incoming error initiator => {:?}", e);
            }
        });

        Ok(())
    }
}

// ----------------------------------------------------------------------------
// Implementierung KademliaP2PAdapter
// ----------------------------------------------------------------------------
impl KademliaP2PAdapter for TcpP2PAdapter {
    fn send_kademlia_msg(&self, addr: SocketAddr, msg: &KademliaMessage) {
        let connections = self.connections.clone();
        let msg_cloned = msg.clone();
        let adapter_ref = self.clone();
        
        // Wir spawnen asynchron, weil Connect + Write blocken könnte.
        tokio::spawn(async move {
            // 1) Falls wir in connections NICHT haben => connect + handshake (Initiator)
            let exists = {
                let lock = connections.lock().unwrap();
                lock.contains_key(&addr)
            };
            if !exists {
                // => connect & handshake
                if let Err(e) = adapter_ref.connect_and_handshake_initiator(addr).await {
                    warn!("connect_and_handshake_initiator({}) => {:?}", addr, e);
                    return;
                }
            }
            // 2) Nun bincode + Noise
            let bin = match bincode::serialize(&msg_cloned) {
                Ok(b) => b,
                Err(e) => {
                    error!("bincode serialize => {:?}", e);
                    return;
                }
            };
            // 3) Hole PeerConnection => noise_session => write_message => => .write_all
            let mut lock = connections.lock().unwrap();
            let pc = match lock.get_mut(&addr) {
                Some(p) => p,
                None => {
                    warn!("PeerConnection zu {} nicht gefunden => aborted", addr);
                    return;
                }
            };
            let mut enc_buf = vec![0u8; bin.len() + 128];
            let len = match pc.noise_session.write_message(&bin, &mut enc_buf) {
                Ok(l) => l,
                Err(e) => {
                    warn!("noise_session.write_message => {:?}", e);
                    // => drop connection
                    lock.remove(&addr);
                    return;
                }
            };
            // => Senden
            if let Err(e) = pc.write_half.write_all(&enc_buf[..len]).await {
                warn!("send_kademlia_msg => write_all error => {:?}", e);
                lock.remove(&addr);
            }
        });
    }

    fn local_address(&self) -> SocketAddr {
        self.local_addr
    }
}

impl Clone for TcpP2PAdapter {
    fn clone(&self) -> Self {
        // Wir klonen nur references:
        Self {
            local_addr: self.local_addr,
            connections: self.connections.clone(),
            listener_handle: self.listener_handle.clone(),
        }
    }
}
