////////////////////////////////////////
// my_dex/src/crypto/multi_sig.rs
////////////////////////////////////////

use threshold_crypto::{SecretKeySet, SecretKeyShare, PublicKeySet, Signature, SignatureShare};
use rand::rngs::OsRng;

/// Erzeugt ein SecretKeySet f�r den angegebenen Schwellenwert und insgesamt n Teilnehmer.
/// Gibt das SecretKeySet, das dazugeh�rige PublicKeySet sowie die SecretKeyShares f�r alle Teilnehmer zur�ck.
pub fn generate_keys(threshold: usize, total: usize) -> (SecretKeySet, PublicKeySet, Vec<SecretKeyShare>) {
    let sk_set = SecretKeySet::random(threshold, &mut OsRng);
    let pk_set = sk_set.public_keys();
    let shares: Vec<SecretKeyShare> = (0..total).map(|i| sk_set.secret_key_share(i)).collect();
    (sk_set, pk_set, shares)
}

/// Signiert eine Nachricht mit einem SecretKeyShare.
pub fn sign_message(share: &SecretKeyShare, message: &[u8]) -> SignatureShare {
    share.sign(message)
}

/// Kombiniert einzelne Signaturanteile zu einer vollst�ndigen Signatur.
/// Gibt None zur�ck, falls nicht gen�gend g�ltige Anteile vorliegen.
pub fn combine_signatures(pk_set: &PublicKeySet, sig_shares: Vec<(usize, SignatureShare)>) -> Option<Signature> {
    pk_set.combine_signatures(sig_shares).ok()
}

/// Verifiziert eine kombinierte Signatur anhand des PublicKeySet und der Nachricht.
pub fn verify_combined_signature(pk_set: &PublicKeySet, message: &[u8], signature: &Signature) -> bool {
    pk_set.verify(signature, message)
}

#[cfg(test)]
mod tests {
    use super::*;
    use threshold_crypto::SecretKeySet;
    use std::str::FromStr;

    #[test]
    fn test_threshold_signature() {
        let threshold = 2;
        let total = 3;
        let (sk_set, pk_set, shares) = generate_keys(threshold, total);
        let message = b"Test message for threshold signature";
        
        // Jeder Teilnehmer signiert die Nachricht
        let sig_shares: Vec<(usize, SignatureShare)> = shares.into_iter().enumerate()
            .map(|(i, share)| (i, sign_message(&share, message)))
            .collect();
        // Kombiniere mindestens t+1 Anteile (hier alle 3, da t = 2)
        let combined = combine_signatures(&pk_set, sig_shares).expect("Should combine signatures");
        assert!(verify_combined_signature(&pk_set, message, &combined));
    }
}
