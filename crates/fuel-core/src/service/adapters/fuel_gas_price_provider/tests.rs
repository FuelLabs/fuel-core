#![allow(non_snake_case)]

use super::*;

#[cfg(test)]
mod producer_gas_price_tests;

#[cfg(test)]
mod tx_pool_gas_price_tests;

#[cfg(test)]
mod graph_ql_gas_price_estimate_tests;

fn build_provider<A>(algorithm: A) -> FuelGasPriceProvider<A>
where
    A: Send + Sync,
{
    let algorithm = SharedGasPriceAlgo::new_with_algorithm(algorithm);
    FuelGasPriceProvider::new(algorithm)
}

#[ignore]
#[test]
fn dummy() {}
