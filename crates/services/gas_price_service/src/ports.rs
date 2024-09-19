use crate::fuel_gas_price_updater::UpdaterMetadata;
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

pub trait GasPriceData: Send + Sync {
    fn get_metadata(
        &self,
        block_height: &BlockHeight,
    ) -> StorageResult<Option<UpdaterMetadata>>;
    fn set_metadata(&mut self, metadata: UpdaterMetadata) -> StorageResult<()>;

    fn latest_height(&self) -> Option<BlockHeight>;

    fn rollback_last_block(&self) -> StorageResult<()>;
}
