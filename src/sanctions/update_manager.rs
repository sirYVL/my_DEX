////////////////////////////////////////////////////
/// my_DEX/src/sanctions/update_manager.rs
////////////////////////////////////////////////////


// Dieses Modul implementiert die automatisierte Aktualisierung der Sanktionsliste
// sowie einen einfachen konsensbasierten Validierungsmechanismus.
// Die konsolidierte Liste wird abgerufen, digital signiert (SHA256-Hash) und
// anhand eines simplen Kriteriums (Hash beginnt mit "00") validiert.
// Bei erfolgreicher Validierung wird die Liste lokal in der Datei "sanctions_list_update.txt" gespeichert.

use crate::sanctions::sanctions_list::SanctionsList;
use std::error::Error;
use sha2::{Sha256, Digest};

/// F�hrt das Update der Sanktionsliste durch.
/// - Ruft die offiziellen Sanktionslisten ab und konsolidiert sie.
/// - Berechnet einen SHA256-Hash der Liste (als digitale Signatur).
/// - Simuliert einen Konsensmechanismus: Ist der Hash g�ltig (z. B. beginnt er mit "00"),
///   wird das Update akzeptiert.
/// - Speichert die konsolidierte Liste lokal in einer Datei.
pub fn update_sanctions_list() -> Result<(), Box<dyn Error>> {
    // Konsolidierte Sanktionsliste abrufen
    let sanctions_list = SanctionsList::consolidate_lists()?;
    
    // Zur Serialisierung: Alle Adressen sortieren und zu einem einzigen String zusammenf�gen
    let mut addresses: Vec<&String> = sanctions_list.addresses.iter().collect();
    addresses.sort();
    let data = addresses.join(",");
    
    // SHA256-Hash der konsolidierten Liste berechnen
    let mut hasher = Sha256::new();
    hasher.update(data.as_bytes());
    let result_hash = hasher.finalize();
    let hash_string = format!("{:x}", result_hash);
    println!("Neuer Hash der Sanktionsliste: {}", hash_string);
    
    // Simulierter Konsensmechanismus:
    // Akzeptiere das Update, wenn der Hash mit "00" beginnt.
    if hash_string.starts_with("00") {
        // Speichere die aktualisierte Liste in der Datei "sanctions_list_update.txt"
        std::fs::write("sanctions_list_update.txt", &data)?;
        println!("Sanktionslisten-Update wurde akzeptiert und gespeichert.");
        Ok(())
    } else {
        println!("Sanktionslisten-Update wurde vom Konsensmechanismus abgelehnt.");
        Err("Konsensvalidierung fehlgeschlagen".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_update_sanctions_list() {
        // Test: Update-Prozess ausf�hren und pr�fen, ob ein Ergebnis zur�ckgeliefert wird.
        match update_sanctions_list() {
            Ok(_) => println!("Update akzeptiert."),
            Err(e) => println!("Update abgelehnt: {}", e),
        }
    }
}
