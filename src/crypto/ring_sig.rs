////////////////////////////////////////
// my_dex/src/crypto/ring_sig.rs
////////////////////////////////////////

use ed25519_dalek::{Keypair, PublicKey, Signature};
use rand::rngs::OsRng;

/// Struktur, die eine Ring-Signatur repr�sentiert.
/// Diese einfache Implementierung erstellt f�r jeden �ffentlichen Schl�ssel im Ring eine Signatur,
/// wobei der Signierer seine echte Signatur erzeugt und f�r die anderen ein Dummy verwendet wird.
/// **Hinweis:** Dies ist nicht sicher und dient nur als Beispiel!
pub struct RingSignature {
    pub signatures: Vec<Signature>,
    pub ring: Vec<PublicKey>,
}

/// Erzeugt eine Ring-Signatur f�r eine Nachricht.
/// Der Parameter `ring` enth�lt die �ffentlichen Schl�ssel aller Teilnehmer.
/// `signer_keypair` ist der Schl�ssel desjenigen, der signiert.
/// **Achtung:** Diese Implementierung ist ein Platzhalter und muss durch einen echten Algorithmus ersetzt werden!
pub fn ring_sign(message: &[u8], ring: &[PublicKey], signer_keypair: &Keypair) -> RingSignature {
    let mut signatures = Vec::new();
    for pk in ring {
        if pk == &signer_keypair.public {
            signatures.push(signer_keypair.sign(message));
        } else {
            // Dummy-Signatur (nicht sicher)
            signatures.push(signer_keypair.sign(b"dummy"));
        }
    }
    RingSignature {
        signatures,
        ring: ring.to_vec(),
    }
}

/// Verifiziert eine Ring-Signatur.
/// **Hinweis:** Diese �berpr�fung ist stark vereinfacht und soll nur demonstrieren, dass mindestens
/// eine Signatur g�ltig ist. F�r eine echte Ring-Signatur-Verifikation ist ein spezialisierter Algorithmus n�tig.
pub fn ring_verify(message: &[u8], ring_sig: &RingSignature) -> bool {
    ring_sig.ring.iter().zip(ring_sig.signatures.iter()).any(|(pk, sig)| {
        pk.verify(message, sig).is_ok()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::Keypair;
    use rand::rngs::OsRng;

    #[test]
    fn test_ring_signature() {
        let mut csprng = OsRng{};
        let keypair1 = Keypair::generate(&mut csprng);
        let keypair2 = Keypair::generate(&mut csprng);
        let ring = vec![keypair1.public, keypair2.public];
        let message = b"Test message for ring signature";

        let ring_sig = ring_sign(message, &ring, &keypair1);
        // In diesem Beispiel pr�fen wir, dass zumindest eine Signatur g�ltig ist.
        assert!(ring_verify(message, &ring_sig));
    }
}
