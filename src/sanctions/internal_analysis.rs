////////////////////////////////////////////////////
/// my_DEX/src/sanctions/internal_analysis.rs
////////////////////////////////////////////////////


// Dieses Modul implementiert einen einfachen Ansatz zur internen Analyse von Transaktionen.
// Es wird anhand des Transaktionsbetrags ermittelt, ob eine Transaktion ungew�hnlich (verd�chtig) ist.

/// Struktur zur Darstellung einer Transaktion.
#[derive(Debug, Clone)]
pub struct Transaction {
    /// Wallet-Adresse oder Identifikator der Transaktion.
    pub address: String,
    /// Betrag der Transaktion (z. B. in Coin-Einheiten).
    pub amount: f64,
}

/// Analysiert eine Liste von Transaktionen und gibt jene zur�ck, die als verd�chtig gelten.
/// Eine Transaktion gilt als verd�chtig, wenn ihr Betrag mehr als 2 Standardabweichungen �ber dem Durchschnitt liegt.
pub fn analyze_transactions(transactions: &[Transaction]) -> Vec<&Transaction> {
    // Berechnung des Durchschnitts
    let sum: f64 = transactions.iter().map(|tx| tx.amount).sum();
    let count = transactions.len() as f64;
    let average = sum / count;

    // Berechnung der Varianz und der Standardabweichung
    let variance = transactions.iter().map(|tx| (tx.amount - average).powi(2)).sum::<f64>() / count;
    let std_dev = variance.sqrt();
    let threshold = average + 2.0 * std_dev;

    // Filter: Transaktionen mit einem Betrag oberhalb des Schwellenwerts gelten als verd�chtig
    transactions.iter().filter(|tx| tx.amount > threshold).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_analyze_transactions() {
        let transactions = vec![
            Transaction { address: "A".to_string(), amount: 100.0 },
            Transaction { address: "B".to_string(), amount: 110.0 },
            Transaction { address: "C".to_string(), amount: 105.0 },
            // Diese Transaktion sollte als verd�chtig erkannt werden
            Transaction { address: "D".to_string(), amount: 1000.0 },
        ];
        let suspicious = analyze_transactions(&transactions);
        assert_eq!(suspicious.len(), 1);
        assert_eq!(suspicious[0].address, "D");
    }
}
