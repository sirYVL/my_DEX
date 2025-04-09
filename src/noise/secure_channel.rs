// my_DEX/src/noise/secure_channel.rs

use anyhow::{Result, anyhow};
use snow::{Builder, params::NoiseParams, Keypair};
use tokio::net::{TcpStream, TcpListener};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tracing::{info, debug, instrument, warn};
use crate::identity::access_control::{AccessPolicy, is_allowed};
use crate::identity::{verify_message};
use crate::utils::aesgcm_utils::SimpleResolver;

#[derive(Clone, Debug)]
pub struct NoiseConfig {
    pub pattern: String, // z. B. "Noise_XX_25519_ChaChaPoly_SHA256"
    pub static_private: Option<Vec<u8>>,
    pub access_policy: Option<AccessPolicy>,
}

/// Minimale Session => Noise
pub struct NoiseSession {
    pub session: snow::Session,
    pub remote_static: Option<Vec<u8>>, // remote static pubkey
}

#[instrument(name="noise_initiator_connect", skip(cfg, addr))]
pub async fn initiator_connect(cfg: &NoiseConfig, addr: &str) -> Result<NoiseSession> {
    let noise_params: NoiseParams = cfg.pattern.parse()?;
    let builder = Builder::new(noise_params);

    let mut session = if let Some(ref sk) = cfg.static_private {
        builder.build_initiator_with_keypair_resolver(Box::new(SimpleResolver(sk.clone())))?
    } else {
        builder.build_initiator()?
    };

    let mut stream = TcpStream::connect(addr).await?;
    info!("Initiator => connected to {}", addr);

    // handshake
    let mut buf = vec![0u8; 1024];
    let len = session.write_message(&[], &mut buf)?;
    stream.write_all(&buf[..len]).await?;

    let mut resp = vec![0u8; 1024];
    let n = stream.read(&mut resp).await?;
    session.read_message(&resp[..n], &mut vec![])?;

    // remote static
    let remote_static = session.get_remote_static().map(|b| b.to_vec());

    if let Some(ap) = &cfg.access_policy {
        if let Some(rs) = &remote_static {
            if !is_allowed(ap, rs) {
                warn!("Remote static key not allowed => close");
                return Err(anyhow!("Remote not in AccessPolicy"));
            }
        }
    }

    info!("Noise handshake done (initiator), remote_static={:?}", remote_static);

    Ok(NoiseSession{ session, remote_static })
}

#[instrument(name="noise_responder_accept", skip(cfg, listener))]
pub async fn responder_accept(cfg: &NoiseConfig, listener: &TcpListener) -> Result<(NoiseSession, TcpStream)> {
    let (mut stream, addr) = listener.accept().await?;
    info!("Responder => accepted from {}", addr);

    let noise_params: NoiseParams = cfg.pattern.parse()?;
    let builder = Builder::new(noise_params);

    let mut session = if let Some(ref sk) = cfg.static_private {
        builder.build_responder_with_keypair_resolver(Box::new(SimpleResolver(sk.clone())))?
    } else {
        builder.build_responder()?
    };

    // read init
    let mut initbuf = vec![0u8; 1024];
    let n = stream.read(&mut initbuf).await?;
    session.read_message(&initbuf[..n], &mut vec![])?;

    // write response
    let mut outbuf = vec![0u8; 1024];
    let len = session.write_message(&[], &mut outbuf)?;
    stream.write_all(&outbuf[..len]).await?;

    let remote_static = session.get_remote_static().map(|b| b.to_vec());
    if let Some(ap) = &cfg.access_policy {
        if let Some(rs) = &remote_static {
            if !is_allowed(ap, rs) {
                warn!("Remote static key not allowed => close");
                return Err(anyhow!("Remote not in AccessPolicy"));
            }
        }
    }

    info!("Noise handshake done (responder), remote_static={:?}", remote_static);

    Ok((NoiseSession{ session, remote_static }, stream))
}
