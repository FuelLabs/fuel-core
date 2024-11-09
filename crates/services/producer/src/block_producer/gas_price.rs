use fuel_core_types::blockchain::header::ConsensusParametersVersion;
use std::sync::Arc;

#[async_trait::async_trait]
/// Interface for retrieving the gas price for a block
pub trait GasPriceProvider {
    /// The gas price for all transactions in the block.
    async fn next_gas_price(&self) -> anyhow::Result<u64>;
}

/// Interface for retrieving the consensus parameters.
#[cfg_attr(feature = "test-helpers", mockall::automock)]
pub trait ConsensusParametersProvider {
    /// Retrieve the consensus parameters for the `version`.
    fn consensus_params_at_version(
        &self,
        version: &ConsensusParametersVersion,
    ) -> anyhow::Result<Arc<fuel_core_types::fuel_tx::ConsensusParameters>>;
}
