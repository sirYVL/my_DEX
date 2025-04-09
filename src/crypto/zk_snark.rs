////////////////////////////////////////
// my_dex/src/crypto/zk_snark.rs
////////////////////////////////////////

extern crate bellman;
extern crate pairing;
extern crate rand;

use bellman::{Circuit, ConstraintSystem, SynthesisError};
use pairing::bls12_381::{Bls12, Fr};
use rand::thread_rng;

/// Ein einfacher Schaltkreis, der das Produkt zweier Eingabewerte berechnet.
pub struct MultiplierCircuit {
    pub a: Option<Fr>,
    pub b: Option<Fr>,
}

impl Circuit<Fr> for MultiplierCircuit {
    fn synthesize<CS: ConstraintSystem<Fr>>(self, cs: &mut CS) -> Result<(), SynthesisError> {
        // Allocieren der Variablen
        let a_val = self.a;
        let a = cs.alloc(|| "a", || a_val.ok_or(SynthesisError::AssignmentMissing))?;
        let b_val = self.b;
        let b = cs.alloc(|| "b", || b_val.ok_or(SynthesisError::AssignmentMissing))?;
        let product_val = self.a.and_then(|a| self.b.map(|b| { let mut prod = a; prod.mul_assign(&b); prod }));
        let product = cs.alloc(|| "a * b", || product_val.ok_or(SynthesisError::AssignmentMissing))?;
        
        // Setze die Multiplikationsbedingung: a * b = product
        cs.enforce(
            || "a * b = product",
            |lc| lc + a,
            |lc| lc + b,
            |lc| lc + product,
        );
        
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bellman::groth16::{generate_random_parameters, create_random_proof, prepare_verifying_key, verify_proof};
    use pairing::bls12_381::{Bls12, Fr};
    use rand::thread_rng;

    #[test]
    fn test_multiplier_circuit() {
        let rng = &mut thread_rng();
        let params = {
            let c = MultiplierCircuit { a: None, b: None };
            generate_random_parameters::<Bls12, _, _>(c, rng).unwrap()
        };

        let pvk = prepare_verifying_key(&params.vk);

        // Testfall: 3 * 11 = 33
        let a = Fr::from_str("3").unwrap();
        let b = Fr::from_str("11").unwrap();
        let mut product = a;
        product.mul_assign(&b);

        let c = MultiplierCircuit {
            a: Some(a),
            b: Some(b),
        };

        let proof = create_random_proof(c, &params, rng).unwrap();

        let public_inputs = vec![product];
        assert!(verify_proof(&pvk, &public_inputs, &proof).unwrap());
    }
}
