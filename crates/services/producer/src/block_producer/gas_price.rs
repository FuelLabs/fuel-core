use fuel_core_types::blockchain::header::ConsensusParametersVersion;
use std::{
    future::Future,
    sync::Arc,
};

/// Interface for retrieving the gas price for a block
pub trait GasPriceProvider {
    /// The gas price for all transactions in the block.
    fn production_gas_price(&self) -> impl Future<Output = anyhow::Result<u64>> + Send;

    fn dry_run_gas_price(&self) -> impl Future<Output = anyhow::Result<u64>> + Send;
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
