#![allow(non_snake_case)]

use super::*;

#[cfg(test)]
mod producer_gas_price_tests;

fn build_provider<A>(
    algorithm: A,
    height: u32,
    price: u64,
    percentage: u16,
) -> FuelGasPriceProvider<A, u32, u64>
where
    A: Send + Sync,
{
    let algorithm = SharedGasPriceAlgo::new_with_algorithm(algorithm);
    let latest_gas_price = UniversalGasPriceProvider::new(height, price, percentage);
    FuelGasPriceProvider::new(algorithm, latest_gas_price)
}

#[ignore]
#[test]
fn dummy() {}
