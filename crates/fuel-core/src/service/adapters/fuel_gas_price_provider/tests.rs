#![allow(non_snake_case)]

use super::*;

#[cfg(test)]
mod producer_gas_price_tests;

#[cfg(test)]
mod tx_pool_gas_price_tests;

#[cfg(test)]
mod graph_ql_gas_price_estimate_tests;

#[derive(Debug, Clone, Copy)]
pub struct TestGasPriceAlgorithm {
    multiply: u64,
}

impl Default for TestGasPriceAlgorithm {
    fn default() -> Self {
        Self { multiply: 2 }
    }
}

impl GasPriceAlgorithm for TestGasPriceAlgorithm {
    fn gas_price(&self, block_bytes: u64) -> u64 {
        self.multiply.saturating_mul(block_bytes)
    }
}

impl GasPriceEstimate for TestGasPriceAlgorithm {
    fn estimate(&self, _block_height: BlockHeight) -> u64 {
        self.multiply.saturating_mul(10_000_000) // Arbitrary fake bytes
    }
}

fn build_provider<A>(algorithm: A) -> FuelGasPriceProvider<A>
where
    A: Send + Sync,
{
    let algorithm = BlockGasPriceAlgo::new(algorithm);
    FuelGasPriceProvider::new(algorithm)
}

#[ignore]
#[test]
fn dummy() {}
