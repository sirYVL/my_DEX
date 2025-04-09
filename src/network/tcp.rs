//////////////////////////////////////////////////
/// my_DEX/src/network/tcp.rs
//////////////////////////////////////////////////

use super::*;
use tokio::net::{TcpListener, TcpStream};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

pub struct NetworkManager {
    pub address: SocketAddr,
}

impl NetworkManager {
    pub async fn new(address: SocketAddr) -> Self {
        NetworkManager { address }
    }

    pub async fn start_listener(&self) -> Result<(), Box<dyn std::error::Error>> {
        let listener = TcpListener::bind(self.address).await?;
        println!("Listening on {}", self.address);
        loop {
            let (mut socket, addr) = listener.accept().await?;
            println!("Neue Verbindung von {}", addr);
            tokio::spawn(async move {
                let mut buf = vec![0u8; 2048];
                loop {
                    let n = match socket.read(&mut buf).await {
                        Ok(n) if n == 0 => break,
                        Ok(n) => n,
                        Err(e) => {
                            println!("Fehler beim Lesen von {}: {}", addr, e);
                            break;
                        }
                    };
                    if let Some(msg) = protocol::deserialize_message(&buf[..n]) {
                        println!("Nachricht von {}: {:?}", addr, msg);
                    }
                }
            });
        }
    }

    pub async fn send_message(&self, addr: SocketAddr, msg: protocol::P2PMessage) -> Result<(), Box<dyn std::error::Error>> {
        let mut stream = TcpStream::connect(addr).await?;
        let data = protocol::serialize_message(&msg);
        stream.write_all(&data).await?;
        Ok(())
    }
}
