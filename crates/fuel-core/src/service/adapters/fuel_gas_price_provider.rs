use crate::{
    fuel_core_graphql_api::ports::GasPriceEstimate as GraphqlGasPriceEstimate,
    service::adapters::fuel_gas_price_provider::ports::{
        GasPriceAlgorithm,
        GasPriceEstimate,
    },
};
use fuel_core_gas_price_service::BlockGasPriceAlgo;
use fuel_core_producer::block_producer::gas_price::GasPriceProvider as ProducerGasPriceProvider;
use fuel_core_txpool::ports::GasPriceProvider as TxPoolGasPricProvider;
use fuel_core_types::{
    fuel_types::BlockHeight,
    services::txpool::{
        Error as TxPoolError,
        Result as TxPoolResult,
    },
};
use std::cmp::Ordering;

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

#[derive(Debug)]
/// Receives the next gas price algorithm via a shared `BlockGasPriceAlgo` instance
pub struct FuelGasPriceProvider<A> {
    algorithm: BlockGasPriceAlgo<A>,
}

impl<A> Clone for FuelGasPriceProvider<A> {
    fn clone(&self) -> Self {
        Self {
            algorithm: self.algorithm.clone(),
        }
    }
}

impl<A> FuelGasPriceProvider<A> {
    pub fn new(algorithm: BlockGasPriceAlgo<A>) -> Self {
        Self { algorithm }
    }
}

impl<A> FuelGasPriceProvider<A>
where
    A: GasPriceAlgorithm + Send + Sync,
{
    async fn execute_algorithm(&self, block_bytes: u64) -> u64 {
        self.algorithm.read().await.1.gas_price(block_bytes)
    }

    async fn try_to_get(
        &self,
        requested_height: BlockHeight,
        block_bytes: u64,
    ) -> Result<u64> {
        let latest_height = self.algorithm.block_height().await;
        match requested_height.cmp(&latest_height) {
            Ordering::Equal => Ok(self.execute_algorithm(block_bytes).await),
            Ordering::Greater => Err(Error::AlgorithmNotUpToDate {
                requested_height,
                latest_height,
            }),
            Ordering::Less => Err(Error::RequestedOldBlockHeight {
                requested_height,
                latest_height,
            }),
        }
    }
}

#[async_trait::async_trait]
impl<A> ProducerGasPriceProvider for FuelGasPriceProvider<A>
where
    A: GasPriceAlgorithm + Send + Sync,
{
    async fn gas_price(
        &self,
        block_height: BlockHeight,
        block_bytes: u64,
    ) -> anyhow::Result<u64> {
        self.try_to_get(block_height, block_bytes)
            .await
            .map_err(Into::into)
    }
}

#[async_trait::async_trait]
impl<A> TxPoolGasPricProvider for FuelGasPriceProvider<A>
where
    A: GasPriceAlgorithm + Send + Sync,
{
    async fn gas_price(
        &self,
        block_height: BlockHeight,
        block_bytes: u64,
    ) -> TxPoolResult<u64> {
        self.try_to_get(block_height, block_bytes)
            .await
            .map_err(|e| TxPoolError::GasPriceNotFound(format!("{e:?}")))
    }
}

#[async_trait::async_trait]
impl<A> GraphqlGasPriceEstimate for FuelGasPriceProvider<A>
where
    A: GasPriceEstimate + Send + Sync,
{
    async fn worst_case_gas_price(&self, height: BlockHeight) -> u64 {
        self.algorithm.read().await.1.estimate(height)
    }
}
