/////////////////////////////////////////////////////
/// my_DEX/src/network/handler.rs
/////////////////////////////////////////////////////

use tokio::net::{TcpListener, TcpStream};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::mpsc;

pub struct P2PNetwork {
    pub peers: Vec<String>,
    pub message_sender: mpsc::Sender<String>,
}

impl P2PNetwork {
    pub async fn start_listener(address: &str, message_sender: mpsc::Sender<String>) -> std::io::Result<()> {
        let listener = TcpListener::bind(address).await?;
        println!("?? P2P-Netzwerk l�uft auf {}", address);

        loop {
            let (mut socket, _) = listener.accept().await?;
            let sender_clone = message_sender.clone();
            tokio::spawn(async move {
                let mut buffer = vec![0; 1024];
                if let Ok(n) = socket.read(&mut buffer).await {
                    if n > 0 {
                        let received_msg = String::from_utf8_lossy(&buffer[..n]).to_string();
                        println!("?? Nachricht erhalten: {}", received_msg);
                        sender_clone.send(received_msg).await.unwrap();
                    }
                }
            });
        }
    }

    pub async fn send_message(&self, peer: &str, message: &str) -> std::io::Result<()> {
        if let Ok(mut stream) = TcpStream::connect(peer).await {
            stream.write_all(message.as_bytes()).await?;
        }
        Ok(())
    }
}
