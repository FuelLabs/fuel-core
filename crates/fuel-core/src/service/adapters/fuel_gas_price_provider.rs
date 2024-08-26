use crate::fuel_core_graphql_api::ports::GasPriceEstimate as GraphqlGasPriceEstimate;
use fuel_core_gas_price_service::{
    GasPriceAlgorithm,
    SharedGasPriceAlgo,
};
use fuel_core_producer::block_producer::gas_price::GasPriceProvider as ProducerGasPriceProvider;
use fuel_core_txpool::{
    ports::GasPriceProvider as TxPoolGasPriceProvider,
    Result as TxPoolResult,
};
use fuel_core_types::fuel_types::BlockHeight;

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
    algorithm: SharedGasPriceAlgo<A>,
}

impl<A> Clone for FuelGasPriceProvider<A> {
    fn clone(&self) -> Self {
        Self {
            algorithm: self.algorithm.clone(),
        }
    }
}

impl<A> FuelGasPriceProvider<A> {
    pub fn new(algorithm: SharedGasPriceAlgo<A>) -> Self {
        Self { algorithm }
    }
}

impl<A> FuelGasPriceProvider<A>
where
    A: GasPriceAlgorithm + Send + Sync,
{
    async fn next_gas_price(&self) -> u64 {
        self.algorithm.next_gas_price().await
    }
}

#[async_trait::async_trait]
impl<A> ProducerGasPriceProvider for FuelGasPriceProvider<A>
where
    A: GasPriceAlgorithm + Send + Sync,
{
    async fn next_gas_price(&self) -> anyhow::Result<u64> {
        Ok(self.next_gas_price().await)
    }
}

#[async_trait::async_trait]
impl<A> TxPoolGasPriceProvider for FuelGasPriceProvider<A>
where
    A: GasPriceAlgorithm + Send + Sync,
{
    async fn next_gas_price(&self) -> TxPoolResult<u64> {
        Ok(self.next_gas_price().await)
    }
}

#[async_trait::async_trait]
impl<A> GraphqlGasPriceEstimate for FuelGasPriceProvider<A>
where
    A: GasPriceAlgorithm + Send + Sync,
{
    async fn worst_case_gas_price(&self, height: BlockHeight) -> Option<u64> {
        Some(self.algorithm.worst_case_gas_price(height).await)
    }
}
