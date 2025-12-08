use crate::service::adapters::UniversalGasPriceProvider;
use anyhow::anyhow;
use fuel_core_gas_price_service::{
    common::gas_price_algorithm::{
        GasPriceAlgorithm,
        SharedGasPriceAlgo,
    },
    v1::algorithm::AlgorithmV1,
};
use fuel_core_producer::block_producer::gas_price::GasPriceProvider as GasPriceProviderTrait;
use fuel_core_types::fuel_types::BlockHeight;

pub type Result<T, E = Error> = std::result::Result<T, E>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(
        "Requested height is too high. Requested: {requested_height}, latest: {latest_height}"
    )]
    AlgorithmNotUpToDate {
        requested_height: BlockHeight,
        latest_height: BlockHeight,
    },
    #[error(
        "Latest block height past requested height. Requested: {requested_height}, latest: {latest_height}"
    )]
    RequestedOldBlockHeight {
        requested_height: BlockHeight,
        latest_height: BlockHeight,
    },
}

#[cfg(test)]
mod tests;

#[derive(Debug)]
pub struct FuelGasPriceProvider<A, Height, GasPrice> {
    algorithm: SharedGasPriceAlgo<A>,
    latest_gas_price: UniversalGasPriceProvider<Height, GasPrice>,
}

impl<A, Height, GasPrice> Clone for FuelGasPriceProvider<A, Height, GasPrice> {
    fn clone(&self) -> Self {
        Self {
            algorithm: self.algorithm.clone(),
            latest_gas_price: self.latest_gas_price.clone(),
        }
    }
}

impl<A, Height, GasPrice> FuelGasPriceProvider<A, Height, GasPrice> {
    pub fn new(
        algorithm: SharedGasPriceAlgo<A>,
        latest_gas_price: UniversalGasPriceProvider<Height, GasPrice>,
    ) -> Self {
        Self {
            algorithm,
            latest_gas_price,
        }
    }
}

impl<A> GasPriceProviderTrait for FuelGasPriceProvider<A, u32, u64>
where
    A: GasPriceAlgorithm + Send + Sync,
{
    fn production_gas_price(&self) -> anyhow::Result<u64> {
        Ok(self.algorithm.next_gas_price())
    }

    fn dry_run_gas_price(&self) -> anyhow::Result<u64> {
        let price = self.latest_gas_price.inner_next_gas_price();
        Ok(price)
    }
}

#[derive(Clone)]
pub enum ProducerGasPriceProvider {
    Full(FuelGasPriceProvider<AlgorithmV1, u32, u64>),
    Disabled(UniversalGasPriceProvider<u32, u64>),
}

impl GasPriceProviderTrait for ProducerGasPriceProvider {
    fn production_gas_price(&self) -> anyhow::Result<u64> {
        match self {
            ProducerGasPriceProvider::Full(provider) => provider.production_gas_price(),
            ProducerGasPriceProvider::Disabled(_) => Err(anyhow!(
                "Block production is not available when gas price algorithm is disabled. \
                 Start the node without --gas-price-algorithm-disabled to enable block production."
            )),
        }
    }

    fn dry_run_gas_price(&self) -> anyhow::Result<u64> {
        match self {
            ProducerGasPriceProvider::Full(provider) => provider.dry_run_gas_price(),
            ProducerGasPriceProvider::Disabled(provider) => {
                Ok(provider.inner_next_gas_price())
            }
        }
    }
}
