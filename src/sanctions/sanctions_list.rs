////////////////////////////////////////////////////
/// my_DEX/src/sanctions/sanctions_list.rs
////////////////////////////////////////////////////

// Dieses Modul ruft offizielle Sanktionslisten ab (beispielhaft OFAC und EU),
// parst die CSV-Daten und konsolidiert die gefundenen Adressen in einer einheitlichen Liste.

use reqwest::blocking::get;
use csv::ReaderBuilder;
use std::error::Error;
use std::collections::HashSet;

/// Struktur zur Speicherung der konsolidierten Sanktionsliste.
#[derive(Debug, Clone)]
pub struct SanctionsList {
    /// Eine Menge eindeutiger Adressen, die als sanktioniert gelten.
    pub addresses: HashSet<String>,
}

impl SanctionsList {
    /// Erzeugt eine neue, leere Sanktionsliste.
    pub fn new() -> Self {
        SanctionsList {
            addresses: HashSet::new(),
        }
    }
    
    /// Ruft die OFAC-Sanktionsliste ab und extrahiert die Adressen.
    /// Hier wird beispielhaft eine CSV-Datei von einer URL abgerufen.
    pub fn fetch_ofac_list() -> Result<Vec<String>, Box<dyn Error>> {
        // Beispiel-URL der OFAC-Sanktionsliste (ggf. anpassen)
        let url = "https://www.treasury.gov/ofac/downloads/sdnlist.csv";
        let response = get(url)?.text()?;
        // CSV-Reader mit Kopfzeile
        let mut rdr = ReaderBuilder::new().has_headers(true).from_reader(response.as_bytes());
        let mut addresses = Vec::new();
        for result in rdr.records() {
            let record = result?;
            // Annahme: Die zweite Spalte enth�lt eine relevante Adresse.
            if let Some(addr) = record.get(1) {
                addresses.push(addr.to_string());
            }
        }
        Ok(addresses)
    }
    
    /// Ruft die EU-Sanktionsliste ab und extrahiert die Adressen.
    pub fn fetch_eu_list() -> Result<Vec<String>, Box<dyn Error>> {
        // Beispiel-URL der EU-Sanktionsliste (ggf. anpassen)
        let url = "https://www.consilium.europa.eu/sanctions/downloads/sanctions_list.csv";
        let response = get(url)?.text()?;
        let mut rdr = ReaderBuilder::new().has_headers(true).from_reader(response.as_bytes());
        let mut addresses = Vec::new();
        for result in rdr.records() {
            let record = result?;
            if let Some(addr) = record.get(1) {
                addresses.push(addr.to_string());
            }
        }
        Ok(addresses)
    }
    
    /// Konsolidiert die aus verschiedenen Quellen abgerufenen Listen zu einer einheitlichen Sanktionsliste.
    pub fn consolidate_lists() -> Result<SanctionsList, Box<dyn Error>> {
        let ofac_addresses = Self::fetch_ofac_list()?;
        let eu_addresses = Self::fetch_eu_list()?;
        
        let mut consolidated = SanctionsList::new();
        // Alle Adressen aus beiden Listen zusammenf�hren
        for addr in ofac_addresses.into_iter().chain(eu_addresses.into_iter()) {
            consolidated.addresses.insert(addr);
        }
        Ok(consolidated)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_consolidate_lists() {
        let result = SanctionsList::consolidate_lists();
        match result {
            Ok(list) => {
                println!("Die konsolidierte Liste enth�lt {} Adressen.", list.addresses.len());
                // Weitere Pr�fungen k�nnen hier erg�nzt werden.
            },
            Err(e) => panic!("Fehler beim Konsolidieren der Listen: {:?}", e),
        }
    }
}
