use crate::service::adapters::fuel_gas_price_provider::ports::GasPriceAlgorithm;
use fuel_core_producer::block_producer::gas_price::{
    GasPriceParams,
    GasPriceProvider as ProducerGasPriceProvider,
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

pub type Result<T, E = Error> = std::result::Result<T, E>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Requested height is too high. Requested: {requested_height}, latest: {latest_height}")]
    AlgorithmNotUpToDate {
        requested_height: BlockHeight,
        latest_height: BlockHeight,
    },
    #[error("Latest block height past requested height. Requested: {requested_height}, latest: {latest_height}")]
    RequestedOldBlockHeight {
        requested_height: BlockHeight,
        latest_height: BlockHeight,
    },
}

#[cfg(test)]
mod tests;

#[derive(Debug, Clone)]
/// Gives the gas price for a given block height, and calculates the gas price if not yet committed.
pub struct FuelGasPriceProvider<A> {
    algorithm: Arc<RwLock<(BlockHeight, A)>>,
}

impl<A> FuelGasPriceProvider<A> {
    pub fn new(algorithm: Arc<RwLock<(BlockHeight, A)>>) -> Self {
        Self { algorithm }
    }
}

impl<A> FuelGasPriceProvider<A>
where
    A: GasPriceAlgorithm,
{
    fn try_to_get(&self, block_height: BlockHeight, block_bytes: u64) -> Result<u64> {
        let guarded_pair = self.algorithm.read().unwrap();
        let height = guarded_pair.0;
        let requested_height = block_height;
        match requested_height.cmp(&height) {
            Ordering::Equal => Ok(guarded_pair.1.gas_price(block_bytes)),
            Ordering::Greater => Err(Error::AlgorithmNotUpToDate {
                requested_height: block_height,
                latest_height: height,
            }),
            Ordering::Less => Err(Error::RequestedOldBlockHeight {
                requested_height: block_height,
                latest_height: height,
            }),
        }
    }
}

impl<A> ProducerGasPriceProvider for FuelGasPriceProvider<A>
where
    A: GasPriceAlgorithm,
{
    fn gas_price(&self, params: GasPriceParams) -> anyhow::Result<u64> {
        self.try_to_get(params.block_height(), params.block_bytes())
            .map_err(Into::into)
    }
}
