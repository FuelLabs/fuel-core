#![allow(non_snake_case)]

use super::*;

#[cfg(test)]
mod producer_gas_price_tests;

#[cfg(test)]
mod tx_pool_gas_price_tests;

#[cfg(test)]
mod graph_ql_gas_price_estimate_tests;

#[derive(Debug, Clone, Copy)]
pub struct SimpleGasPriceAlgorithm {
    multiply: u64,
}

impl Default for SimpleGasPriceAlgorithm {
    fn default() -> Self {
        Self { multiply: 2 }
    }
}

impl GasPriceAlgorithm for SimpleGasPriceAlgorithm {
    fn gas_price(&self, block_bytes: u64) -> u64 {
        self.multiply * block_bytes
    }
}

impl GasPriceEstimate for SimpleGasPriceAlgorithm {
    fn estimate(&self, _block_height: BlockHeight) -> u64 {
        self.multiply * 10_000_000 // Arbitrary fake bytes
    }
}

fn build_provider<A>(latest_height: BlockHeight, algorithm: A) -> FuelGasPriceProvider<A>
where
    A: Send + Sync,
{
    let algorithm = BlockGasPriceAlgo::new(latest_height, algorithm);
    FuelGasPriceProvider::new(algorithm)
}

#[ignore]
#[test]
fn dummy() {}
