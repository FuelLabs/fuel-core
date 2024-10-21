use crate::common::{
    updater_metadata::UpdaterMetadata,
    utils::Result,
};
use fuel_core_storage::Result as StorageResult;
use fuel_core_types::{
    blockchain::block::Block,
    fuel_tx::Transaction,
    fuel_types::BlockHeight,
};

pub trait L2Data: Send + Sync {
    fn latest_height(&self) -> StorageResult<BlockHeight>;
    fn get_block(
        &self,
        height: &BlockHeight,
    ) -> StorageResult<Option<Block<Transaction>>>;
}

pub trait MetadataStorage: Send + Sync {
    fn get_metadata(&self, block_height: &BlockHeight)
        -> Result<Option<UpdaterMetadata>>;
    fn set_metadata(&mut self, metadata: &UpdaterMetadata) -> Result<()>;
}

/// Provides the latest block height.
/// This is used to determine the latest block height that has been processed by the gas price service.
/// We need this to fetch the gas price data for the latest block.
pub trait GasPriceData: Send + Sync {
    fn latest_height(&self) -> Option<BlockHeight>;
}

pub struct GasPriceServiceConfig {
    pub min_gas_price: u64,
    pub starting_gas_price: u64,
    pub gas_price_change_percent: u64,
    pub gas_price_threshold_percent: u64,
}

impl GasPriceServiceConfig {
    pub fn new(
        min_gas_price: u64,
        starting_gas_price: u64,
        gas_price_change_percent: u64,
        gas_price_threshold_percent: u64,
    ) -> Self {
        Self {
            min_gas_price,
            starting_gas_price,
            gas_price_change_percent,
            gas_price_threshold_percent,
        }
    }
}
