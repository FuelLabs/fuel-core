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
    last: u64,
    multiply: u64,
}

impl Default for TestGasPriceAlgorithm {
    fn default() -> Self {
        Self {
            last: 100,
            multiply: 2,
        }
    }
}

impl GasPriceAlgorithm for TestGasPriceAlgorithm {
    fn last_gas_price(&self) -> u64 {
        self.last
    }

    fn next_gas_price(&self, block_bytes: u64) -> u64 {
        self.multiply.saturating_mul(block_bytes)
    }

    fn worst_case_gas_price(&self, _block_height: BlockHeight) -> u64 {
        self.multiply.saturating_mul(10_000_000) // Arbitrary fake bytes
    }
}

fn build_provider<A>(algorithm: A) -> FuelGasPriceProvider<A>
where
    A: Send + Sync,
{
    let algorithm = SharedGasPriceAlgo::new(algorithm);
    FuelGasPriceProvider::new(algorithm)
}

#[ignore]
#[test]
fn dummy() {}
