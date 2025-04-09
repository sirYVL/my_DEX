///////////////////////////////////////////////////////////
// my_DEX/src/storage/ipfs_storage.rs
///////////////////////////////////////////////////////////


use ipfs_api::IpfsClient;
use std::fs::File;
use std::io::Read;
use futures::TryStreamExt;

/// F�gt eine Datei (z.?B. ein Audit-Log) zu IPFS hinzu und gibt den resultierenden Hash zur�ck.
pub async fn add_file_to_ipfs(file_path: &str) -> Result<String, Box<dyn std::error::Error>> {
    // Erzeuge einen Standard-IPFS-Client (Verbindung zu localhost:5001)
    let client = IpfsClient::default();

    // Lese den Inhalt der Datei in einen Puffer
    let mut file = File::open(file_path)?;
    let mut data = Vec::new();
    file.read_to_end(&mut data)?;

    // F�ge die Datei zu IPFS hinzu
    let res = client.add(std::io::Cursor::new(data)).await?;
    Ok(res.hash)
}

/// Liest den Inhalt einer �ber IPFS gespeicherten Datei anhand ihres Hashes.
pub async fn cat_file_from_ipfs(hash: &str) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let client = IpfsClient::default();
    let mut stream = client.cat(hash);
    let mut result = Vec::new();
    while let Some(chunk) = stream.try_next().await? {
        result.extend_from_slice(&chunk);
    }
    Ok(result)
}
