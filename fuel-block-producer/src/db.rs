use anyhow::Result;
use fuel_core_interfaces::{
    common::fuel_storage::StorageAsRef,
    model::{
        BlockHeight,
        FuelBlockDb,
    },
};
use fuel_database::{
    not_found,
    tables::FuelBlocks,
    Database,
};
use std::borrow::Cow;

pub trait BlockProducerDatabase: Sync + Send {
    /// fetch previously committed block at given height
    fn get_block(&self, fuel_height: BlockHeight) -> Result<Option<Cow<FuelBlockDb>>>;

    /// Fetch the current block height
    fn current_block_height(&self) -> Result<BlockHeight>;
}

impl BlockProducerDatabase for Database {
    fn get_block(&self, fuel_height: BlockHeight) -> Result<Option<Cow<FuelBlockDb>>> {
        let id = self
            .get_block_id(fuel_height)?
            .ok_or(not_found!("BlockId"))?;
        self.storage::<FuelBlocks>().get(&id).map_err(Into::into)
    }

    fn current_block_height(&self) -> Result<BlockHeight> {
        self.get_block_height()
            .map(|h| h.unwrap_or_default())
            .map_err(Into::into)
    }
}
