use anyhow::Result;
use fuel_core_interfaces::{
    common::prelude::StorageInspect,
    db::{
        KvStoreError,
        Messages,
    },
    model::{
        BlockHeight,
        FuelBlockDb,
    },
};
use std::borrow::Cow;

pub trait BlockProducerDatabase:
    Sync + Send + StorageInspect<Messages, Error = KvStoreError>
{
    /// fetch previously committed block at given height
    fn get_block(&self, fuel_height: BlockHeight) -> Result<Option<Cow<FuelBlockDb>>>;

    /// Fetch the current block height
    fn current_block_height(&self) -> Result<BlockHeight>;
}
