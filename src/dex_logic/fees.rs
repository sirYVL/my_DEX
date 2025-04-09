// my_dex/src/dex_logic/fees.rs
//
// Definiere die Logik für das Fee-Handling
// jetzt mit tieferer Instrumentierung

use tracing::{info, instrument};

#[derive(Clone, Debug)]
pub struct FeeDistribution {
    pub founder_percent: f64,
    pub dev_percent: f64,
    pub node_percent: f64,
}

impl FeeDistribution {
    pub fn new() -> Self {
        // z.B. Founder=50%, Dev=30%, Node=20% (der 0.1% Gesamtsumme)
        Self {
            founder_percent: 0.5,
            dev_percent: 0.3,
            node_percent: 0.2,
        }
    }
}

#[derive(Clone, Debug)]
pub struct FeeOutput {
    pub founder_fee: f64,
    pub dev_fee: f64,
    pub node_fee: f64,
}

#[instrument(name="calc_fee_distribution", skip(distribution))]
pub fn calc_fee_distribution(total_fee: f64, distribution: &FeeDistribution) -> FeeOutput {
    let f = total_fee * distribution.founder_percent;
    let d = total_fee * distribution.dev_percent;
    let n = total_fee * distribution.node_percent;

    let out = FeeOutput {
        founder_fee: f,
        dev_fee: d,
        node_fee: n,
    };
    info!("Calc fee dist => total_fee={}, founder={}, dev={}, node={}",
          total_fee, out.founder_fee, out.dev_fee, out.node_fee);
    out
}
