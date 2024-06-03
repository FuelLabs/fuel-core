use crate::service::adapters::fuel_gas_price_provider::ports::GasPriceAlgorithm;
use fuel_core_producer::block_producer::gas_price::{
    GasPriceParams,
    GasPriceProvider,
};
use fuel_core_types::fuel_types::BlockHeight;
use std::{
    cmp::Ordering,
    sync::{
        Arc,
        RwLock,
    },
};

pub mod ports;

use ports::Error;

#[cfg(test)]
mod tests;

#[allow(dead_code)]
/// Gives the gas price for a given block height, and calculates the gas price if not yet committed.
pub struct FuelGasPriceProvider<A> {
    algorithm: Arc<RwLock<(BlockHeight, A)>>,
}

impl<A> FuelGasPriceProvider<A> {
    pub fn new(algorithm: Arc<RwLock<(BlockHeight, A)>>) -> Self {
        Self { algorithm }
    }
}

impl<A> GasPriceProvider for FuelGasPriceProvider<A>
where
    A: GasPriceAlgorithm,
{
    fn gas_price(&self, params: GasPriceParams) -> anyhow::Result<u64> {
        let guarded_pair = self.algorithm.read().unwrap();
        let height = guarded_pair.0;
        let requested_height = params.block_height();
        match requested_height.cmp(&height) {
            Ordering::Equal => Ok(guarded_pair.1.gas_price(params.block_bytes())),
            Ordering::Greater => Err(Error::AlgorithmNotUpToDate {
                requested_height: params.block_height(),
                latest_height: height,
            }
            .into()),
            Ordering::Less => Err(Error::RequestedOldBlockHeight {
                requested_height: params.block_height(),
                latest_height: height,
            }
            .into()),
        }
    }
}
