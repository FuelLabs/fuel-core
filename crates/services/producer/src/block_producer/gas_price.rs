use fuel_core_types::{
    blockchain::header::ConsensusParametersVersion,
    fuel_types::BlockHeight,
};
use std::sync::Arc;

/// The parameters required to retrieve the gas price for a block
pub struct GasPriceParams {
    block_height: BlockHeight,
}

impl GasPriceParams {
    /// Create a new `GasPriceParams` instance
    pub fn new(block_height: BlockHeight) -> Self {
        Self { block_height }
    }

    pub fn block_height(&self) -> BlockHeight {
        self.block_height
    }
}

impl From<BlockHeight> for GasPriceParams {
    fn from(block_height: BlockHeight) -> Self {
        Self { block_height }
    }
}

/// Interface for retrieving the gas price for a block
pub trait GasPriceProvider {
    /// The gas price for all transactions in the block.
    fn gas_price(&self, params: GasPriceParams) -> Option<u64>;
}

/// Interface for retrieving the consensus parameters.
#[cfg_attr(feature = "test-helpers", mockall::automock)]
pub trait ConsensusParametersProvider {
    /// Retrieve the consensus parameters for the `version`.
    fn consensus_params_at_version(
        &self,
        version: &ConsensusParametersVersion,
    ) -> Arc<fuel_core_types::fuel_tx::ConsensusParameters>;
}
