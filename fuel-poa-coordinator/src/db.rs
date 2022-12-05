use anyhow::Result;
use fuel_core_interfaces::{
    common::prelude::StorageAsMut,
    model::{
        BlockHeight,
        BlockId,
        FuelBlockConsensus,
    },
};
use fuel_database::tables::SealedBlockConsensus;

pub trait BlockDb: Send + Sync {
    fn block_height(&self) -> Result<BlockHeight>;

    /// Returns error if already sealed
    fn seal_block(
        &mut self,
        block_id: BlockId,
        consensus: FuelBlockConsensus,
    ) -> Result<()>;
}

impl BlockDb for fuel_database::Database {
    fn block_height(&self) -> Result<BlockHeight> {
        Ok(self.get_block_height()?.unwrap_or_default())
    }

    fn seal_block(
        &mut self,
        block_id: BlockId,
        consensus: FuelBlockConsensus,
    ) -> Result<()> {
        self.storage::<SealedBlockConsensus>()
            .insert(&block_id.into(), &consensus)
            .map(|_| ())
            .map_err(Into::into)
    }
}
