#![allow(non_snake_case)]

use super::*;
use crate::service::adapters::BlockHeight;

#[cfg(test)]
mod producer_gas_price_tests;

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

fn build_provider<A>(
    latest_height: BlockHeight,
    algorithm: A,
) -> FuelGasPriceProvider<A> {
    let algorithm = Arc::new(RwLock::new((latest_height, algorithm)));
    FuelGasPriceProvider::new(algorithm)
}

#[ignore]
#[test]
fn dummy() {}
