use fuel_core_storage::Result as StorageResult;
use fuel_core_types::{
    blockchain::block::Block,
    fuel_tx::Transaction,
    fuel_types::BlockHeight,
};

use crate::{
    common::{
        updater_metadata::UpdaterMetadata,
        utils::Result,
    },
    v1::uninitialized_task::fuel_storage_unrecorded_blocks::AsUnrecordedBlocks,
};

pub trait L2Data: Send + Sync {
    fn latest_height(&self) -> StorageResult<BlockHeight>;
    fn get_block(
        &self,
        height: &BlockHeight,
    ) -> StorageResult<Option<Block<Transaction>>>;
}

pub trait SetMetadataStorage: Send + Sync {
    fn set_metadata(&mut self, metadata: &UpdaterMetadata) -> Result<()>;
}

pub trait GetMetadataStorage: Send + Sync {
    fn get_metadata(&self, block_height: &BlockHeight)
        -> Result<Option<UpdaterMetadata>>;
}

pub trait SetLatestRecordedHeight: Send + Sync {
    /// Set the latest L2 block height which has been committed to the DA layer
    fn set_recorded_height(&mut self, recorded_height: BlockHeight) -> Result<()>;
}

pub trait GetLatestRecordedHeight: Send + Sync {
    /// Get the most recent L2 block that has been committed to DA
    fn get_recorded_height(&self) -> Result<Option<BlockHeight>>;
}

pub trait GasPriceServiceAtomicStorage
where
    Self: 'static,
    Self: Send + Sync,
    Self: GetMetadataStorage + GetLatestRecordedHeight,
{
    type Transaction<'a>: AsUnrecordedBlocks
        + SetMetadataStorage
        + GetMetadataStorage
        + SetLatestRecordedHeight
        + GetLatestRecordedHeight
    where
        Self: 'a;

    fn begin_transaction(&mut self) -> Result<Self::Transaction<'_>>;

    fn commit_transaction(transaction: Self::Transaction<'_>) -> Result<()>;
}

/// Provides the latest block height.
/// This is used to determine the latest block height that has been processed by the gas price service.
/// We need this to fetch the gas price data for the latest block.
pub trait GasPriceData: Send + Sync {
    fn latest_height(&self) -> Option<BlockHeight>;
}
